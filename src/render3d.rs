//! Petit moteur de rendu logiciel (z-buffer + ombrage) pour l'aperçu 3D.

use crate::mesh::Tri;

pub fn render(tris: &[Tri], w: usize, h: usize, yaw: f32, pitch: f32, dark: bool) -> Vec<u8> {
    let bg: [u8; 3] = if dark { [22, 24, 28] } else { [236, 238, 242] };
    let mut px = vec![0u8; w * h * 4];
    for i in 0..w * h {
        px[4 * i..4 * i + 4].copy_from_slice(&[bg[0], bg[1], bg[2], 255]);
    }
    if tris.is_empty() {
        return px;
    }
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for t in tris {
        for v in t {
            for k in 0..3 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
    }
    let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
    let (cy, sy) = (yaw.cos(), yaw.sin());
    let (cp, sp) = (pitch.cos(), pitch.sin());
    let xf = |v: [f32; 3]| -> [f32; 3] {
        let (x, y, z) = (v[0] - c[0], v[1] - c[1], v[2] - c[2]);
        // rotation autour de Z (lacet) puis autour de X (inclinaison)
        let (x1, y1) = (x * cy - y * sy, x * sy + y * cy);
        let (y2, z2) = (y1 * cp - z * sp, y1 * sp + z * cp);
        [x1, y2, z2]
    };
    let diag = ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt();
    let scale = 0.9 * (w.min(h) as f32) / diag.max(1.0);
    let mut zb = vec![f32::MIN; w * h];
    let light = {
        let l = [-0.35f32, -0.75, 0.55];
        let n = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
        [l[0] / n, l[1] / n, l[2] / n]
    };
    for t in tris {
        let a = xf(t[0]);
        let b = xf(t[1]);
        let cc = xf(t[2]);
        // normale dans le repère vue
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [cc[0] - a[0], cc[1] - a[1], cc[2] - a[2]];
        let mut n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if l == 0.0 {
            continue;
        }
        n = [n[0] / l, n[1] / l, n[2] / l];
        // vue depuis -Y (l'observateur regarde vers +Y), z vers le haut
        let facing = -n[1];
        if facing < 0.0 {
            n = [-n[0], -n[1], -n[2]];
        }
        let diffuse = (n[0] * light[0] + n[1] * light[1] + n[2] * light[2]).max(0.0);
        // teinte selon l'altitude réelle : fonds d'empreintes plus sombres que le dessus
        let zr = ((t[0][2] + t[1][2] + t[2][2]) / 3.0 - lo[2]) / (hi[2] - lo[2]).max(1e-3);
        let shade = (0.30 + 0.70 * diffuse) * (0.55 + 0.45 * zr);
        let base: [f32; 3] = if facing >= 0.0 { [236.0, 160.0, 80.0] } else { [120.0, 130.0, 145.0] };
        let col = [(base[0] * shade).min(255.0), (base[1] * shade).min(255.0), (base[2] * shade).min(255.0)];
        let s = |p: [f32; 3]| [w as f32 / 2.0 + p[0] * scale, h as f32 / 2.0 - p[2] * scale, -p[1]];
        let (p0, p1, p2) = (s(a), s(b), s(cc));
        let minx = p0[0].min(p1[0]).min(p2[0]).floor().max(0.0) as i32;
        let maxx = p0[0].max(p1[0]).max(p2[0]).ceil().min(w as f32 - 1.0) as i32;
        let miny = p0[1].min(p1[1]).min(p2[1]).floor().max(0.0) as i32;
        let maxy = p0[1].max(p1[1]).max(p2[1]).ceil().min(h as f32 - 1.0) as i32;
        let area = (p1[0] - p0[0]) * (p2[1] - p0[1]) - (p1[1] - p0[1]) * (p2[0] - p0[0]);
        if area.abs() < 1e-6 {
            continue;
        }
        for y in miny..=maxy {
            for x in minx..=maxx {
                let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                let w0 = ((p1[0] - fx) * (p2[1] - fy) - (p1[1] - fy) * (p2[0] - fx)) / area;
                let w1 = ((p2[0] - fx) * (p0[1] - fy) - (p2[1] - fy) * (p0[0] - fx)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let z = w0 * p0[2] + w1 * p1[2] + w2 * p2[2];
                let i = y as usize * w + x as usize;
                if z > zb[i] {
                    zb[i] = z;
                    px[4 * i] = col[0] as u8;
                    px[4 * i + 1] = col[1] as u8;
                    px[4 * i + 2] = col[2] as u8;
                }
            }
        }
    }
    px
}
