//! Traitement d'image : détection de la feuille, redressement, segmentation des objets,
//! dilatation (tolérance) et extraction des contours.

use image::{GrayImage, Luma, Rgb, RgbImage, imageops};
use imageproc::contours::{BorderType, find_contours};
use imageproc::geometric_transformations::{Border, Interpolation, Projection, warp_into};
use imageproc::geometry::{approximate_polygon_dp, convex_hull};
use imageproc::point::Point;
use imageproc::region_labelling::{Connectivity, connected_components};

pub type P2 = [f64; 2];

// ---------------------------------------------------------------------------
// Détection de la feuille
// ---------------------------------------------------------------------------

/// Cherche les 4 coins de la feuille (zone claire la plus grande) dans la photo.
/// Retourne les coins dans l'ordre : haut-gauche, haut-droit, bas-droit, bas-gauche,
/// le premier côté (0→1) étant le grand côté de la feuille.
pub fn detect_paper(img: &RgbImage) -> Option<[P2; 4]> {
    let (w, h) = img.dimensions();
    let scale = (1000.0 / w.max(h) as f64).min(1.0);
    let sw = ((w as f64 * scale).round() as u32).max(1);
    let sh = ((h as f64 * scale).round() as u32).max(1);
    let small = imageops::resize(img, sw, sh, imageops::FilterType::Triangle);
    let gray = imageops::grayscale(&small);
    let gray = imageproc::filter::gaussian_blur_f32(&gray, 1.5);
    let lvl = imageproc::contrast::otsu_level(&gray);
    let mask = GrayImage::from_fn(sw, sh, |x, y| {
        Luma([if gray.get_pixel(x, y)[0] > lvl { 255 } else { 0 }])
    });
    let labels = connected_components(&mask, Connectivity::Four, Luma([0u8]));
    let mut counts: Vec<u32> = Vec::new();
    for p in labels.pixels() {
        let l = p[0] as usize;
        if l == 0 {
            continue;
        }
        if counts.len() <= l {
            counts.resize(l + 1, 0);
        }
        counts[l] += 1;
    }
    let best = counts
        .iter()
        .enumerate()
        .max_by_key(|(_, c)| **c)
        .map(|(i, _)| i as u32)?;
    if best == 0 || (counts[best as usize] as f64) < 0.02 * (sw * sh) as f64 {
        return None;
    }
    // Cadre vide de 1 px : sinon une feuille qui touche le bord de la photo n'a pas de contour extérieur.
    let comp = GrayImage::from_fn(sw, sh, |x, y| {
        let border = x == 0 || y == 0 || x == sw - 1 || y == sh - 1;
        Luma([if !border && labels.get_pixel(x, y)[0] == best { 255 } else { 0 }])
    });
    let contours = find_contours::<i32>(&comp);
    let outer = contours
        .iter()
        .filter(|c| c.border_type == BorderType::Outer)
        .max_by_key(|c| c.points.len())?;
    let hull = convex_hull(outer.points.clone());
    let quad = max_area_quad(&hull)?;
    let pts = quad.map(|p| [p[0] / scale, p[1] / scale]);
    Some(order_corners(pts))
}

pub fn poly_area(p: &[P2]) -> f64 {
    let mut a = 0.0;
    for i in 0..p.len() {
        let j = (i + 1) % p.len();
        a += p[i][0] * p[j][1] - p[j][0] * p[i][1];
    }
    a * 0.5
}

/// Quadrilatère d'aire maximale inscrit dans l'enveloppe convexe.
fn max_area_quad(hull: &[Point<i32>]) -> Option<[P2; 4]> {
    if hull.len() < 4 {
        return None;
    }
    let perim: f64 = (0..hull.len())
        .map(|i| {
            let a = hull[i];
            let b = hull[(i + 1) % hull.len()];
            (((a.x - b.x).pow(2) + (a.y - b.y).pow(2)) as f64).sqrt()
        })
        .sum();
    // Simplifie l'enveloppe pour garder une recherche exhaustive rapide.
    let mut eps = 0.002 * perim;
    let mut simp = approximate_polygon_dp(hull, eps, true);
    while simp.len() > 40 {
        eps *= 1.5;
        simp = approximate_polygon_dp(hull, eps, true);
    }
    if simp.len() < 4 {
        simp = hull.to_vec();
    }
    let p: Vec<P2> = simp.iter().map(|q| [q.x as f64, q.y as f64]).collect();
    let n = p.len();
    if n < 4 {
        return None;
    }
    let mut best = (0.0, [0usize; 4]);
    for i in 0..n {
        for j in i + 1..n {
            for k in j + 1..n {
                for l in k + 1..n {
                    let a = poly_area(&[p[i], p[j], p[k], p[l]]).abs();
                    if a > best.0 {
                        best = (a, [i, j, k, l]);
                    }
                }
            }
        }
    }
    let b = best.1;
    Some([p[b[0]], p[b[1]], p[b[2]], p[b[3]]])
}

/// Ordonne les coins dans le sens horaire à l'écran en commençant en haut à gauche,
/// puis tourne pour que le côté 0→1 soit le grand côté.
pub fn order_corners(mut c: [P2; 4]) -> [P2; 4] {
    let cx = c.iter().map(|p| p[0]).sum::<f64>() / 4.0;
    let cy = c.iter().map(|p| p[1]).sum::<f64>() / 4.0;
    c.sort_by(|a, b| {
        let aa = (a[1] - cy).atan2(a[0] - cx);
        let bb = (b[1] - cy).atan2(b[0] - cx);
        aa.partial_cmp(&bb).unwrap()
    });
    // départ : coin minimisant x+y
    let start = (0..4)
        .min_by(|&i, &j| (c[i][0] + c[i][1]).partial_cmp(&(c[j][0] + c[j][1])).unwrap())
        .unwrap();
    c.rotate_left(start);
    let d = |a: P2, b: P2| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
    let e02 = d(c[0], c[1]) + d(c[2], c[3]);
    let e13 = d(c[1], c[2]) + d(c[3], c[0]);
    if e13 > e02 {
        c.rotate_left(1);
    }
    c
}

/// Redresse la feuille : image de (long × court) mm à `ppm` pixels par mm.
pub fn rectify(img: &RgbImage, corners: &[P2; 4], long_mm: f64, short_mm: f64, ppm: f64) -> RgbImage {
    let w = (long_mm * ppm).round() as u32;
    let h = (short_mm * ppm).round() as u32;
    let from = corners.map(|p| (p[0] as f32, p[1] as f32));
    let to = [(0.0, 0.0), (w as f32, 0.0), (w as f32, h as f32), (0.0, h as f32)];
    let mut out = RgbImage::new(w, h);
    if let Some(proj) = Projection::from_control_points(from, to) {
        warp_into(img, proj, Interpolation::Bilinear, Border::Constant(Rgb([255, 255, 255])), &mut out);
    }
    out
}

// ---------------------------------------------------------------------------
// Distance euclidienne exacte (Felzenszwalb & Huttenlocher)
// ---------------------------------------------------------------------------

const INF: f64 = 1e12;

fn edt_1d(f: &[f64], d: &mut [f64], v: &mut [usize], z: &mut [f64]) {
    let n = f.len();
    let mut k = 0usize;
    v[0] = 0;
    z[0] = -INF;
    z[1] = INF;
    for q in 1..n {
        let qf = q as f64;
        let mut s;
        loop {
            let p = v[k];
            let pf = p as f64;
            s = ((f[q] + qf * qf) - (f[p] + pf * pf)) / (2.0 * qf - 2.0 * pf);
            if s <= z[k] {
                k -= 1;
            } else {
                break;
            }
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = INF;
    }
    k = 0;
    for q in 0..n {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let p = v[k];
        let dq = q as f64 - p as f64;
        d[q] = dq * dq + f[p];
    }
}

/// Carré de la distance (en pixels) au pixel `true` le plus proche.
pub fn edt(mask: &[bool], w: usize, h: usize) -> Vec<f64> {
    let mut g: Vec<f64> = mask.iter().map(|&m| if m { 0.0 } else { INF }).collect();
    let n = w.max(h);
    let mut f = vec![0.0; n];
    let mut d = vec![0.0; n];
    let mut v = vec![0usize; n];
    let mut z = vec![0.0; n + 1];
    for x in 0..w {
        for y in 0..h {
            f[y] = g[y * w + x];
        }
        edt_1d(&f[..h], &mut d[..h], &mut v, &mut z);
        for y in 0..h {
            g[y * w + x] = d[y];
        }
    }
    for y in 0..h {
        f[..w].copy_from_slice(&g[y * w..y * w + w]);
        edt_1d(&f[..w], &mut d[..w], &mut v, &mut z);
        g[y * w..y * w + w].copy_from_slice(&d[..w]);
    }
    g
}

pub fn dilate(mask: &[bool], w: usize, h: usize, r: f64) -> Vec<bool> {
    if r <= 0.0 {
        return mask.to_vec();
    }
    let r2 = r * r;
    edt(mask, w, h).into_iter().map(|d| d <= r2).collect()
}

pub fn erode(mask: &[bool], w: usize, h: usize, r: f64) -> Vec<bool> {
    if r <= 0.0 {
        return mask.to_vec();
    }
    let inv: Vec<bool> = mask.iter().map(|m| !m).collect();
    dilate(&inv, w, h, r).into_iter().map(|m| !m).collect()
}

// ---------------------------------------------------------------------------
// Segmentation des objets sur la feuille redressée
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct SegParams {
    /// Un pixel est « objet » s'il est plus sombre que `dark` × fond local.
    pub dark: f32,
    /// … ou si sa saturation dépasse ce seuil (objets colorés).
    pub sat: f32,
    /// Marge ignorée au bord de la feuille (mm).
    pub margin_mm: f64,
    /// Surface minimale d'un objet (mm²).
    pub min_area_mm2: f64,
    /// Rayon de fermeture morphologique (mm) pour recoller les morceaux.
    pub close_mm: f64,
    /// Bouche les trous internes des objets.
    pub fill_holes: bool,
    /// Mode « bords nets » : les objets sont délimités par leurs arêtes franches,
    /// ce qui ignore les ombres (bords flous). Sinon : simple seuil sur l'assombrissement.
    pub edges: bool,
    /// Force minimale d'une arête (Canny, seuil haut).
    pub edge_thr: f32,
    /// Mode bords : pixels très sombres toujours considérés comme objet (× fond).
    pub seed_dark: f32,
}

impl Default for SegParams {
    fn default() -> Self {
        Self {
            dark: 0.78,
            sat: 0.30,
            margin_mm: 4.0,
            min_area_mm2: 40.0,
            close_mm: 1.5,
            fill_holes: true,
            edges: true,
            edge_thr: 60.0,
            seed_dark: 0.45,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ObjInfo {
    /// Étiquette dans `Segmentation::labels`.
    pub label: u32,
    /// Point situé sur l'objet, le plus proche de son centre (mm, repère image).
    pub sample_mm: P2,
    pub area_mm2: f64,
    /// Centre en mm (repère image : x vers la droite, y vers le bas).
    pub center_mm: P2,
}

pub struct Segmentation {
    pub w: usize,
    pub h: usize,
    pub labels: Vec<u32>,
    pub objects: Vec<ObjInfo>,
}

/// Estimation du fond (papier) : moyenne par bloc de 1 mm, filtre max, flou.
fn background(gray: &[f32], w: usize, h: usize, ppm: f64, radius_mm: f64) -> impl Fn(usize, usize) -> f32 {
    let step = ppm.max(1.0) as usize;
    let bw = w.div_ceil(step);
    let bh = h.div_ceil(step);
    let mut small = vec![0f32; bw * bh];
    for by in 0..bh {
        for bx in 0..bw {
            let (mut s, mut n) = (0f32, 0f32);
            for y in by * step..((by + 1) * step).min(h) {
                for x in bx * step..((bx + 1) * step).min(w) {
                    s += gray[y * w + x];
                    n += 1.0;
                }
            }
            small[by * bw + bx] = s / n.max(1.0);
        }
    }
    let r = radius_mm.round() as isize;
    let pass = |src: &Vec<f32>, horiz: bool, op_max: bool| -> Vec<f32> {
        let mut out = vec![0f32; bw * bh];
        for y in 0..bh as isize {
            for x in 0..bw as isize {
                let mut acc = if op_max { 0f32 } else { 0f32 };
                let mut n = 0f32;
                for t in -r..=r {
                    let (xx, yy) = if horiz { (x + t, y) } else { (x, y + t) };
                    if xx < 0 || yy < 0 || xx >= bw as isize || yy >= bh as isize {
                        continue;
                    }
                    let v = src[yy as usize * bw + xx as usize];
                    if op_max {
                        acc = acc.max(v);
                    } else {
                        acc += v;
                        n += 1.0;
                    }
                }
                out[y as usize * bw + x as usize] = if op_max { acc } else { acc / n.max(1.0) };
            }
        }
        out
    };
    let m = pass(&small, true, true);
    let m = pass(&m, false, true);
    let m = pass(&m, true, false);
    let m = pass(&m, false, false);
    move |x: usize, y: usize| {
        let fx = ((x as f64 + 0.5) / step as f64 - 0.5).clamp(0.0, (bw - 1) as f64);
        let fy = ((y as f64 + 0.5) / step as f64 - 0.5).clamp(0.0, (bh - 1) as f64);
        let x0 = fx.floor() as usize;
        let y0 = fy.floor() as usize;
        let x1 = (x0 + 1).min(bw - 1);
        let y1 = (y0 + 1).min(bh - 1);
        let tx = (fx - x0 as f64) as f32;
        let ty = (fy - y0 as f64) as f32;
        let a = m[y0 * bw + x0] * (1.0 - tx) + m[y0 * bw + x1] * tx;
        let b = m[y1 * bw + x0] * (1.0 - tx) + m[y1 * bw + x1] * tx;
        a * (1.0 - ty) + b * ty
    }
}

/// Retouche manuelle du masque des objets : trait de pinceau (mm, repère feuille, y vers le bas).
#[derive(Clone, Debug, PartialEq)]
pub struct BrushStroke {
    /// Vrai : ajoute de la matière à l'objet ; faux : en retire.
    pub add: bool,
    /// Rayon du pinceau (mm).
    pub radius_mm: f64,
    pub pts: Vec<P2>,
}

/// Peint un trait (suite de segments à bouts ronds) dans le masque.
fn paint_stroke(mask: &mut [bool], w: usize, h: usize, ppm: f64, s: &BrushStroke) {
    if s.pts.is_empty() {
        return;
    }
    let r = (s.radius_mm * ppm).max(0.5);
    let px: Vec<P2> = s.pts.iter().map(|q| [q[0] * ppm, q[1] * ppm]).collect();
    let segs: Vec<(P2, P2)> = if px.len() == 1 { vec![(px[0], px[0])] } else { px.windows(2).map(|w| (w[0], w[1])).collect() };
    for (a, b) in segs {
        let x0 = (a[0].min(b[0]) - r).floor().max(0.0) as usize;
        let y0 = (a[1].min(b[1]) - r).floor().max(0.0) as usize;
        let x1 = ((a[0].max(b[0]) + r).ceil().max(0.0) as usize).min(w.saturating_sub(1));
        let y1 = ((a[1].max(b[1]) + r).ceil().max(0.0) as usize).min(h.saturating_sub(1));
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len2 = dx * dx + dy * dy;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (fx, fy) = (x as f64 + 0.5, y as f64 + 0.5);
                let t = if len2 > 0.0 { (((fx - a[0]) * dx + (fy - a[1]) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
                let (ex, ey) = (fx - a[0] - t * dx, fy - a[1] - t * dy);
                if ex * ex + ey * ey <= r * r {
                    mask[y * w + x] = s.add;
                }
            }
        }
    }
}

/// Applique les retouches dans l'ordre où elles ont été faites. Les zones fermées par un
/// trait d'ajout sont remplies si `fill` est vrai (un contour tracé à la main devient plein).
pub fn apply_strokes(mask: &mut [bool], w: usize, h: usize, ppm: f64, strokes: &[BrushStroke], fill: bool) {
    if strokes.is_empty() {
        return;
    }
    for s in strokes {
        paint_stroke(mask, w, h, ppm, s);
    }
    if fill && strokes.iter().any(|s| s.add) {
        fill_holes(mask, w, h);
        // Les retraits restent prioritaires : un trou gommé ne doit pas être rebouché.
        for s in strokes.iter().filter(|s| !s.add) {
            paint_stroke(mask, w, h, ppm, s);
        }
    }
}

/// Détection complète, sans retouche (utilisée pour les essais).
#[allow(dead_code)]
pub fn segment(img: &RgbImage, ppm: f64, p: &SegParams) -> Segmentation {
    let mask = segment_mask(img, ppm, p);
    label_objects(&mask, img.width() as usize, img.height() as usize, ppm, p.min_area_mm2)
}

/// Masque brut des objets (étape coûteuse, mise en cache).
pub fn segment_mask(img: &RgbImage, ppm: f64, p: &SegParams) -> Vec<bool> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let gray: Vec<f32> = img
        .pixels()
        .map(|px| 0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32)
        .collect();
    let bg = background(&gray, w, h, ppm, 30.0);
    let margin = (p.margin_mm * ppm) as usize;
    let edge_map = if p.edges {
        let g8 = GrayImage::from_raw(w as u32, h as u32, gray.iter().map(|v| *v as u8).collect()).unwrap();
        Some(imageproc::edges::canny(&g8, p.edge_thr * 0.4, p.edge_thr))
    } else {
        None
    };
    let px = img.as_raw();
    let mut mask = vec![false; w * h];
    for y in margin..h.saturating_sub(margin) {
        for x in margin..w.saturating_sub(margin) {
            let i = y * w + x;
            let (r, g, b) = (px[3 * i] as f32, px[3 * i + 1] as f32, px[3 * i + 2] as f32);
            let mx = r.max(g).max(b);
            let mn = r.min(g).min(b);
            let sat = if mx > 0.0 { (mx - mn) / mx } else { 0.0 };
            let colored = mx > 40.0 && sat > p.sat;
            mask[i] = match &edge_map {
                Some(e) => e.as_raw()[i] > 0 || gray[i] < bg(x, y) * p.seed_dark || colored,
                None => gray[i] < bg(x, y) * p.dark || colored,
            };
        }
    }
    let cr = p.close_mm * ppm;
    let mut mask = if p.edges {
        // Fermeture avec remplissage : les contours fermés deviennent des objets pleins,
        // les ombres (sans arête nette à l'extérieur) restent vides.
        let mut d = dilate(&mask, w, h, cr);
        fill_holes(&mut d, w, h);
        let m = erode(&d, w, h, cr);
        // Ouverture : supprime les traits isolés (arêtes hors objets).
        let or = 0.6 * ppm;
        dilate(&erode(&m, w, h, or), w, h, or)
    } else {
        let m = erode(&dilate(&mask, w, h, cr), w, h, cr);
        let or = 0.3 * ppm;
        dilate(&erode(&m, w, h, or), w, h, or)
    };
    if p.fill_holes {
        fill_holes(&mut mask, w, h);
    }
    mask
}

/// Composantes connexes du masque → objets numérotés (les trop petits sont écartés).
pub fn label_objects(mask: &[bool], w: usize, h: usize, ppm: f64, min_area_mm2: f64) -> Segmentation {
    // Composantes connexes
    let gimg = GrayImage::from_fn(w as u32, h as u32, |x, y| Luma([if mask[y as usize * w + x as usize] { 255 } else { 0 }]));
    let lab = connected_components(&gimg, Connectivity::Eight, Luma([0u8]));
    let mut labels: Vec<u32> = lab.pixels().map(|p| p[0]).collect();
    let nlab = labels.iter().copied().max().unwrap_or(0) as usize;
    let mut area = vec![0usize; nlab + 1];
    let mut sx = vec![0f64; nlab + 1];
    let mut sy = vec![0f64; nlab + 1];
    for y in 0..h {
        for x in 0..w {
            let l = labels[y * w + x] as usize;
            area[l] += 1;
            sx[l] += x as f64;
            sy[l] += y as f64;
        }
    }
    // Pixel de chaque objet le plus proche de son centre (reste sur l'objet même s'il est en C).
    let mut best = vec![(f64::MAX, 0usize, 0usize); nlab + 1];
    for y in 0..h {
        for x in 0..w {
            let l = labels[y * w + x] as usize;
            if l == 0 || area[l] == 0 {
                continue;
            }
            let (cx, cy) = (sx[l] / area[l] as f64, sy[l] / area[l] as f64);
            let d = (x as f64 - cx).powi(2) + (y as f64 - cy).powi(2);
            if d < best[l].0 {
                best[l] = (d, x, y);
            }
        }
    }
    let px_area = 1.0 / (ppm * ppm);
    let mut keep = vec![false; nlab + 1];
    let mut objects = Vec::new();
    for l in 1..=nlab {
        let a = area[l] as f64 * px_area;
        if a >= min_area_mm2 {
            keep[l] = true;
            objects.push(ObjInfo {
                label: l as u32,
                sample_mm: [(best[l].1 as f64 + 0.5) / ppm, (best[l].2 as f64 + 0.5) / ppm],
                area_mm2: a,
                center_mm: [sx[l] / area[l] as f64 / ppm, sy[l] / area[l] as f64 / ppm],
            });
        }
    }
    for l in labels.iter_mut() {
        if !keep[*l as usize] {
            *l = 0;
        }
    }
    Segmentation { w, h, labels, objects }
}

/// Remplit les zones de fond non reliées au bord de l'image.
fn fill_holes(mask: &mut [bool], w: usize, h: usize) {
    let mut outside = vec![false; w * h];
    let mut stack = Vec::new();
    for x in 0..w {
        stack.push(x);
        stack.push((h - 1) * w + x);
    }
    for y in 0..h {
        stack.push(y * w);
        stack.push(y * w + w - 1);
    }
    while let Some(i) = stack.pop() {
        if outside[i] || mask[i] {
            continue;
        }
        outside[i] = true;
        let (x, y) = (i % w, i / w);
        if x > 0 { stack.push(i - 1); }
        if x + 1 < w { stack.push(i + 1); }
        if y > 0 { stack.push(i - w); }
        if y + 1 < h { stack.push(i + w); }
    }
    for i in 0..w * h {
        if !outside[i] {
            mask[i] = true;
        }
    }
}

// ---------------------------------------------------------------------------
// Empreintes (poches) et contours
// ---------------------------------------------------------------------------

/// Masque des poches : objets retenus dilatés de `tol_mm` + encoches pour les doigts.
pub fn pocket_mask(
    seg: &Segmentation,
    excluded: &[u32],
    ppm: f64,
    tol_mm: f64,
    notches: &[P2],
    notch_d_mm: f64,
) -> Vec<bool> {
    let (w, h) = (seg.w, seg.h);
    let base: Vec<bool> = seg.labels.iter().map(|&l| l != 0 && !excluded.contains(&l)).collect();
    // +0.5 px : les contours passent par le centre des pixels de bord.
    let mut m = dilate(&base, w, h, tol_mm * ppm + 0.5);
    let r = notch_d_mm * 0.5 * ppm;
    for n in notches {
        let (cx, cy) = (n[0] * ppm, n[1] * ppm);
        let x0 = ((cx - r).floor().max(0.0)) as usize;
        let x1 = ((cx + r).ceil() as usize).min(w.saturating_sub(1));
        let y0 = ((cy - r).floor().max(0.0)) as usize;
        let y1 = ((cy + r).ceil() as usize).min(h.saturating_sub(1));
        for y in y0..=y1 {
            for x in x0..=x1 {
                if (x as f64 - cx).powi(2) + (y as f64 - cy).powi(2) <= r * r {
                    m[y * w + x] = true;
                }
            }
        }
    }
    m
}

/// Contours des poches en mm, repère « monde » (x à droite, y vers le haut).
#[derive(Clone, Debug, Default)]
pub struct Outline {
    pub rings: Vec<Vec<P2>>,
    pub is_hole: Vec<bool>,
    pub parent: Vec<Option<usize>>,
    /// Profondeur propre de chaque empreinte (None = profondeur par défaut du plateau).
    pub depth: Vec<Option<f64>>,
    /// Hauteur de la feuille en mm (pour revenir au repère image).
    pub sheet_h_mm: f64,
}

pub fn extract_outline(mask: &[bool], w: usize, h: usize, ppm: f64, simplify_mm: f64, smooth: usize) -> Outline {
    // Bord de 1 px vide pour que les contours restent fermés.
    let img = GrayImage::from_fn(w as u32, h as u32, |x, y| {
        let inside = x > 0 && y > 0 && (x as usize) < w - 1 && (y as usize) < h - 1;
        Luma([if inside && mask[y as usize * w + x as usize] { 255 } else { 0 }])
    });
    let contours = find_contours::<i32>(&img);
    let mut out = Outline { sheet_h_mm: h as f64 / ppm, ..Default::default() };
    let mut map: Vec<Option<usize>> = vec![None; contours.len()];
    for (ci, c) in contours.iter().enumerate() {
        // parent ignoré si lui-même rejeté
        let parent = match c.parent {
            Some(p) => match map[p] {
                Some(np) => Some(np),
                None => continue,
            },
            None => None,
        };
        let simp = approximate_polygon_dp(&c.points, (simplify_mm * ppm).max(0.5), true);
        if simp.len() < 3 {
            continue;
        }
        let mut ring: Vec<P2> = simp
            .iter()
            .map(|p| [p.x as f64 / ppm, (h as f64 - p.y as f64) / ppm])
            .collect();
        for _ in 0..smooth {
            ring = chaikin(&ring);
        }
        if poly_area(&ring).abs() < 0.5 {
            continue;
        }
        map[ci] = Some(out.rings.len());
        out.rings.push(ring);
        out.is_hole.push(c.border_type == BorderType::Hole);
        out.parent.push(parent);
        out.depth.push(None);
    }
    out
}

fn chaikin(r: &[P2]) -> Vec<P2> {
    let n = r.len();
    let mut o = Vec::with_capacity(n * 2);
    for i in 0..n {
        let a = r[i];
        let b = r[(i + 1) % n];
        o.push([0.75 * a[0] + 0.25 * b[0], 0.75 * a[1] + 0.25 * b[1]]);
        o.push([0.25 * a[0] + 0.75 * b[0], 0.25 * a[1] + 0.75 * b[1]]);
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke(add: bool, r: f64, pts: &[P2]) -> BrushStroke {
        BrushStroke { add, radius_mm: r, pts: pts.to_vec() }
    }

    #[test]
    fn retouches_ajout_et_gomme() {
        let (w, h, ppm) = (400, 300, 4.0); // 100 × 75 mm
        let mut mask = vec![false; w * h];
        // Rectangle de 40 × 10 mm à partir de (10, 10) mm
        for y in 40..80 {
            for x in 40..200 {
                mask[y * w + x] = true;
            }
        }
        let base = label_objects(&mask, w, h, ppm, 5.0);
        assert_eq!(base.objects.len(), 1);

        // Un trait de gomme vertical coupe le rectangle en deux objets.
        let mut m = mask.clone();
        apply_strokes(&mut m, w, h, ppm, &[stroke(false, 1.5, &[[30.0, 5.0], [30.0, 25.0]])], true);
        assert_eq!(label_objects(&m, w, h, ppm, 5.0).objects.len(), 2);

        // Un contour fermé tracé à la main devient un objet plein.
        let mut m = mask.clone();
        let carre = [[60.0, 40.0], [80.0, 40.0], [80.0, 60.0], [60.0, 60.0], [60.0, 40.0]];
        apply_strokes(&mut m, w, h, ppm, &[stroke(true, 1.0, &carre)], true);
        let seg = label_objects(&m, w, h, ppm, 5.0);
        assert_eq!(seg.objects.len(), 2);
        let a = seg.objects.iter().map(|o| o.area_mm2).fold(0.0, f64::max);
        assert!(a > 20.0 * 20.0 * 0.95, "carré rempli : {a} mm²");

        // Une gomme dans ce carré reste un trou même avec le remplissage.
        let mut m = mask.clone();
        apply_strokes(&mut m, w, h, ppm, &[stroke(true, 1.0, &carre), stroke(false, 3.0, &[[70.0, 50.0]])], true);
        assert!(!m[(50.0 * ppm) as usize * w + (70.0 * ppm) as usize]);
    }
}
