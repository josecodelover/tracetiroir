//! Génération du plateau 3D (simple ou Gridfinity) et exports STL / SVG / DXF.

use crate::vision::{Outline, P2};
use std::io::Write;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlateMode {
    Simple,
    Gridfinity,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlateParams {
    pub mode: PlateMode,
    /// Profondeur des empreintes (mm).
    pub depth: f64,
    /// Épaisseur sous les empreintes (mm).
    pub floor: f64,
    /// Bordure autour des empreintes (mm).
    pub margin: f64,
    /// Rayon des coins du plateau simple (mm).
    pub corner_r: f64,
    /// Gridfinity : nombre minimal d'unités (x, y, hauteur). 0 = automatique.
    pub gf_min: [u32; 3],
    /// Plaque simple : dimensions imposées (largeur, longueur en mm). 0 = automatique.
    pub size: [f64; 2],
}

impl Default for PlateParams {
    fn default() -> Self {
        Self { mode: PlateMode::Simple, depth: 15.0, floor: 2.0, margin: 6.0, corner_r: 4.0, gf_min: [0, 0, 0], size: [0.0, 0.0] }
    }
}

pub const GF_PITCH: f64 = 42.0;
pub const GF_BASE_H: f64 = 4.75;

pub type Tri = [[f32; 3]; 3];

/// Résultat : maillage + infos de dimensionnement.
pub struct Plate {
    pub tris: Vec<Tri>,
    pub size: [f64; 3],
    pub info: String,
    /// Contour du plateau et empreintes, dans le repère du plateau (mm).
    pub plate_ring: Vec<P2>,
    pub pockets: Outline,
}

// ---------------------------------------------------------------------------
// Outils géométriques
// ---------------------------------------------------------------------------

fn area(p: &[P2]) -> f64 {
    let mut a = 0.0;
    for i in 0..p.len() {
        let j = (i + 1) % p.len();
        a += p[i][0] * p[j][1] - p[j][0] * p[i][1];
    }
    a * 0.5
}

fn ccw(mut r: Vec<P2>) -> Vec<P2> {
    if area(&r) < 0.0 {
        r.reverse();
    }
    r
}

pub fn point_in(p: P2, r: &[P2]) -> bool {
    let mut inside = false;
    let n = r.len();
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (r[i], r[j]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Rectangle à coins arrondis, sens trigonométrique, `seg` segments par coin.
pub fn rounded_rect(cx: f64, cy: f64, w: f64, h: f64, r: f64, seg: usize) -> Vec<P2> {
    let r = r.min(w / 2.0).min(h / 2.0);
    let (hx, hy) = (w / 2.0, h / 2.0);
    if r < 1e-6 {
        return vec![[cx + hx, cy - hy], [cx + hx, cy + hy], [cx - hx, cy + hy], [cx - hx, cy - hy]];
    }
    let corners = [(hx - r, hy - r, 0.0), (-hx + r, hy - r, 90.0), (-hx + r, -hy + r, 180.0), (hx - r, -hy + r, 270.0)];
    let mut out = Vec::with_capacity(4 * (seg + 1));
    for (ox, oy, a0) in corners {
        for s in 0..=seg {
            let a = (a0 + 90.0 * s as f64 / seg as f64).to_radians();
            out.push([cx + ox + r * a.cos(), cy + oy + r * a.sin()]);
        }
    }
    out
}

struct MeshBuilder {
    tris: Vec<Tri>,
}

impl MeshBuilder {
    fn tri(&mut self, a: [f64; 3], b: [f64; 3], c: [f64; 3]) {
        let f = |p: [f64; 3]| [p[0] as f32, p[1] as f32, p[2] as f32];
        self.tris.push([f(a), f(b), f(c)]);
    }

    /// Face horizontale (polygone + trous) à l'altitude z, normale vers le haut ou le bas.
    fn cap(&mut self, outer: &[P2], holes: &[&[P2]], z: f64, up: bool) {
        let mut data: Vec<f64> = Vec::new();
        let mut hole_idx = Vec::new();
        for p in outer {
            data.extend_from_slice(p);
        }
        for h in holes {
            hole_idx.push(data.len() / 2);
            for p in *h {
                data.extend_from_slice(p);
            }
        }
        let Ok(idx) = earcutr::earcut(&data, &hole_idx, 2) else { return };
        let v = |i: usize| [data[2 * i], data[2 * i + 1]];
        for t in idx.chunks_exact(3) {
            let (a, mut b, mut c) = (v(t[0]), v(t[1]), v(t[2]));
            let s = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            if (s > 0.0) != up {
                std::mem::swap(&mut b, &mut c);
            }
            self.tri([a[0], a[1], z], [b[0], b[1], z], [c[0], c[1], z]);
        }
    }

    /// Paroi verticale le long d'un anneau CCW. `outward` : normale vers l'extérieur du polygone.
    fn wall(&mut self, ring: &[P2], z0: f64, z1: f64, outward: bool) {
        let n = ring.len();
        for i in 0..n {
            let a = ring[i];
            let b = ring[(i + 1) % n];
            let (a0, a1) = ([a[0], a[1], z0], [a[0], a[1], z1]);
            let (b0, b1) = ([b[0], b[1], z0], [b[0], b[1], z1]);
            if outward {
                self.tri(a0, b0, b1);
                self.tri(a0, b1, a1);
            } else {
                self.tri(a0, b1, b0);
                self.tri(a0, a1, b1);
            }
        }
    }

    /// Raccorde deux anneaux CCW de même nombre de points (normale extérieure).
    fn loft(&mut self, r0: &[P2], z0: f64, r1: &[P2], z1: f64) {
        let n = r0.len();
        for i in 0..n {
            let j = (i + 1) % n;
            let a = [r0[i][0], r0[i][1], z0];
            let b = [r0[j][0], r0[j][1], z0];
            let c = [r1[j][0], r1[j][1], z1];
            let d = [r1[i][0], r1[i][1], z1];
            self.tri(a, b, c);
            self.tri(a, c, d);
        }
    }
}

// ---------------------------------------------------------------------------
// Construction du plateau
// ---------------------------------------------------------------------------

pub fn build_plate(outline: &Outline, p: &PlateParams) -> Result<Plate, String> {
    // Anneaux normalisés CCW
    let rings: Vec<Vec<P2>> = outline.rings.iter().map(|r| ccw(r.clone())).collect();
    let outers: Vec<usize> = (0..rings.len()).filter(|&i| !outline.is_hole[i]).collect();
    if outers.is_empty() {
        return Err("Aucun objet détecté".into());
    }
    let (mut minx, mut miny, mut maxx, mut maxy) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for &i in &outers {
        for q in &rings[i] {
            minx = minx.min(q[0]);
            maxx = maxx.max(q[0]);
            miny = miny.min(q[1]);
            maxy = maxy.max(q[1]);
        }
    }
    let (sx, sy) = (maxx - minx, maxy - miny);
    // Profondeur de chaque empreinte (la plus profonde fixe la hauteur du plateau).
    let ring_depth: Vec<f64> = (0..rings.len())
        .map(|i| outline.depth.get(i).copied().flatten().unwrap_or(p.depth).max(0.2))
        .collect();
    let depth = outers.iter().map(|&i| ring_depth[i]).fold(p.depth.max(0.2), f64::max);

    // Dimensions et position du plateau
    let (pw, ph, total_h, radius, info);
    match p.mode {
        PlateMode::Simple => {
            let (aw, ah) = (sx + 2.0 * p.margin, sy + 2.0 * p.margin);
            for (k, need) in [(0, aw), (1, ah)] {
                if p.size[k] > 0.0 && p.size[k] + 1e-6 < need {
                    return Err(format!(
                        "Plateau trop petit : il faut au moins {:.0} × {:.0} mm avec cette bordure.",
                        aw.ceil(),
                        ah.ceil()
                    ));
                }
            }
            pw = if p.size[0] > 0.0 { p.size[0] } else { aw };
            ph = if p.size[1] > 0.0 { p.size[1] } else { ah };
            total_h = depth + p.floor.max(0.4);
            radius = p.corner_r;
            info = format!("Plateau {:.1} × {:.1} × {:.1} mm", pw, ph, total_h);
        }
        PlateMode::Gridfinity => {
            let nx = (((sx + 2.0 * p.margin + 0.5) / GF_PITCH).ceil() as u32).max(p.gf_min[0]).max(1);
            let ny = (((sy + 2.0 * p.margin + 0.5) / GF_PITCH).ceil() as u32).max(p.gf_min[1]).max(1);
            let need = GF_BASE_H + p.floor.max(0.4) + depth;
            let nz = ((need / 7.0).ceil() as u32).max(p.gf_min[2]).max(1);
            pw = nx as f64 * GF_PITCH - 0.5;
            ph = ny as f64 * GF_PITCH - 0.5;
            total_h = nz as f64 * 7.0;
            radius = 3.75;
            info = format!(
                "Bac Gridfinity {}×{}×{}u ({:.1} × {:.1} × {:.1} mm, sans rebord d'empilement)",
                nx, ny, nz, pw, ph, total_h
            );
        }
    }
    // Translation : empreintes centrées dans le plateau, origine au coin.
    let dx = pw / 2.0 - (minx + maxx) / 2.0;
    let dy = ph / 2.0 - (miny + maxy) / 2.0;
    let rings: Vec<Vec<P2>> = rings.iter().map(|r| r.iter().map(|q| [q[0] + dx, q[1] + dy]).collect()).collect();
    let plate_ring = rounded_rect(pw / 2.0, ph / 2.0, pw, ph, radius, 8);

    // Les empreintes doivent rester dans le plateau.
    for &i in &outers {
        if rings[i].iter().any(|q| !point_in(*q, &plate_ring)) {
            return Err("Une empreinte dépasse du plateau : augmentez la bordure.".into());
        }
    }

    let t = total_h;
    let zf_of = |i: usize| t - ring_depth[i];
    let mut m = MeshBuilder { tris: Vec::new() };

    // Face supérieure : plateau moins les empreintes de premier niveau.
    let top_holes: Vec<&[P2]> = outers
        .iter()
        .filter(|&&i| outline.parent[i].is_none())
        .map(|&i| rings[i].as_slice())
        .collect();
    m.cap(&plate_ring, &top_holes, t, true);
    for i in 0..rings.len() {
        if outline.is_hole[i] {
            // Îlot (trou dans une empreinte) : reste à pleine hauteur.
            let inner: Vec<&[P2]> = (0..rings.len())
                .filter(|&k| !outline.is_hole[k] && outline.parent[k] == Some(i))
                .map(|k| rings[k].as_slice())
                .collect();
            m.cap(&rings[i], &inner, t, true);
            let zf = outline.parent[i].map(zf_of).unwrap_or(t - depth);
            m.wall(&rings[i], zf, t, true);
        } else {
            // Fond de l'empreinte.
            let holes: Vec<&[P2]> = (0..rings.len())
                .filter(|&k| outline.is_hole[k] && outline.parent[k] == Some(i))
                .map(|k| rings[k].as_slice())
                .collect();
            m.cap(&rings[i], &holes, zf_of(i), true);
            m.wall(&rings[i], zf_of(i), t, false);
        }
    }

    match p.mode {
        PlateMode::Simple => {
            m.cap(&plate_ring, &[], 0.0, false);
            m.wall(&plate_ring, 0.0, t, true);
        }
        PlateMode::Gridfinity => {
            if t - depth < GF_BASE_H + 0.3 {
                return Err("Empreinte trop profonde pour la hauteur Gridfinity".into());
            }
            m.wall(&plate_ring, GF_BASE_H, t, true);
            let nx = ((pw + 0.5) / GF_PITCH).round() as usize;
            let ny = ((ph + 0.5) / GF_PITCH).round() as usize;
            // Profil de base Gridfinity (légèrement réduit de 0,05 mm pour garder une
            // face inférieure valide entre les pieds).
            let levels = [(0.0, 35.6, 0.8), (0.8, 37.2, 1.6), (2.6, 37.2, 1.6), (GF_BASE_H, 41.4, 3.7)];
            let mut pad_tops = Vec::new();
            for ix in 0..nx {
                for iy in 0..ny {
                    let cx = GF_PITCH * (ix as f64 + 0.5) - 0.25;
                    let cy = GF_PITCH * (iy as f64 + 0.5) - 0.25;
                    let rs: Vec<Vec<P2>> =
                        levels.iter().map(|&(_, s, r)| rounded_rect(cx, cy, s, s, r, 6)).collect();
                    m.cap(&rs[0], &[], 0.0, false);
                    for k in 0..levels.len() - 1 {
                        m.loft(&rs[k], levels[k].0, &rs[k + 1], levels[k + 1].0);
                    }
                    pad_tops.push(rs[levels.len() - 1].clone());
                }
            }
            let holes: Vec<&[P2]> = pad_tops.iter().map(|r| r.as_slice()).collect();
            m.cap(&plate_ring, &holes, GF_BASE_H, false);
        }
    }

    let pockets = Outline {
        rings,
        is_hole: outline.is_hole.clone(),
        parent: outline.parent.clone(),
        depth: outline.depth.clone(),
        sheet_h_mm: outline.sheet_h_mm,
    };
    Ok(Plate { tris: m.tris, size: [pw, ph, t], info, plate_ring, pockets })
}

// ---------------------------------------------------------------------------
// Exports
// ---------------------------------------------------------------------------

pub fn write_stl(path: &std::path::Path, tris: &[Tri]) -> std::io::Result<()> {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut header = [0u8; 80];
    let h = b"TraceTiroir - plateau a empreintes";
    header[..h.len()].copy_from_slice(h);
    f.write_all(&header)?;
    f.write_all(&(tris.len() as u32).to_le_bytes())?;
    for t in tris {
        let u = [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]];
        let v = [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]];
        let mut n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if l > 0.0 {
            n = [n[0] / l, n[1] / l, n[2] / l];
        }
        for x in n.iter().chain(t.iter().flatten()) {
            f.write_all(&x.to_le_bytes())?;
        }
        f.write_all(&[0, 0])?;
    }
    f.flush()
}

pub fn write_svg(path: &std::path::Path, plate: &Plate) -> std::io::Result<()> {
    let (w, h) = (plate.size[0], plate.size[1]);
    let ring_path = |r: &[P2]| {
        let mut s = String::new();
        for (i, q) in r.iter().enumerate() {
            s += &format!("{}{:.3},{:.3} ", if i == 0 { "M" } else { "L" }, q[0], h - q[1]);
        }
        s + "Z "
    };
    let mut d = String::new();
    for r in &plate.pockets.rings {
        d += &ring_path(r);
    }
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w:.2}mm" height="{h:.2}mm" viewBox="0 0 {w:.3} {h:.3}">
<path d="{}" fill="none" stroke="black" stroke-width="0.2"/>
<path d="{}" fill="#888" fill-rule="evenodd" stroke="red" stroke-width="0.2"/>
</svg>
"##,
        ring_path(&plate.plate_ring),
        d
    );
    std::fs::write(path, svg)
}

pub fn write_dxf(path: &std::path::Path, plate: &Plate) -> std::io::Result<()> {
    let mut s = String::from("0\nSECTION\n2\nENTITIES\n");
    let mut poly = |r: &[P2], layer: &str| {
        s += &format!("0\nPOLYLINE\n8\n{layer}\n66\n1\n70\n1\n");
        for q in r {
            s += &format!("0\nVERTEX\n8\n{layer}\n10\n{:.4}\n20\n{:.4}\n", q[0], q[1]);
        }
        s += &format!("0\nSEQEND\n8\n{layer}\n");
    };
    poly(&plate.plate_ring, "PLATEAU");
    for r in &plate.pockets.rings {
        poly(r, "EMPREINTES");
    }
    s += "0\nENDSEC\n0\nEOF\n";
    std::fs::write(path, s)
}
