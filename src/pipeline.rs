//! Enchaînement des étapes avec mise en cache : on ne recalcule que ce qui a changé.

use crate::mesh::{Plate, PlateParams, build_plate};
use crate::vision::{self, Outline, P2, SegParams, Segmentation};
use image::RgbImage;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Paper {
    pub name: &'static str,
    pub long: f64,
    pub short: f64,
}

pub const PAPERS: [Paper; 5] = [
    Paper { name: "A4 (297 × 210)", long: 297.0, short: 210.0 },
    Paper { name: "A3 (420 × 297)", long: 420.0, short: 297.0 },
    Paper { name: "A5 (210 × 148)", long: 210.0, short: 148.0 },
    Paper { name: "Letter US (279,4 × 215,9)", long: 279.4, short: 215.9 },
    Paper { name: "Legal US (355,6 × 215,9)", long: 355.6, short: 215.9 },
];

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub paper: Paper,
    /// Résolution de travail (pixels par mm).
    pub ppm: f64,
    pub seg: SegParams,
    /// Jeu autour des objets (mm).
    pub tol_mm: f64,
    pub simplify_mm: f64,
    pub smooth: usize,
    pub notch_d: f64,
    pub plate: PlateParams,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            paper: PAPERS[0],
            ppm: 8.0,
            seg: SegParams::default(),
            tol_mm: 1.0,
            simplify_mm: 0.15,
            smooth: 1,
            notch_d: 22.0,
            plate: PlateParams::default(),
        }
    }
}

pub struct Project {
    pub photo: RgbImage,
    pub corners: [P2; 4],
    /// Points (mm, repère feuille) désignant les objets à ignorer.
    pub excluded_pts: Vec<P2>,
    /// Encoches pour les doigts (mm, repère feuille, y vers le bas).
    pub notches: Vec<P2>,
    /// Profondeurs propres à certains objets : (point sur l'objet en mm, profondeur en mm).
    pub depth_pts: Vec<(P2, f64)>,

    pub rectified: Option<RgbImage>,
    pub seg: Option<Segmentation>,
    pub excluded_labels: Vec<u32>,
    pub outline: Option<Outline>,
    pub plate: Option<Result<Plate, String>>,
    keys: [String; 4],
}

impl Project {
    /// Vrai si les coins de la feuille sont collés aux bords de la photo :
    /// la feuille est probablement coupée et l'échelle douteuse.
    pub fn sheet_cropped(&self) -> bool {
        let (w, h) = (self.photo.width() as f64, self.photo.height() as f64);
        let near = |c: P2| c[0] < 6.0 || c[1] < 6.0 || c[0] > w - 7.0 || c[1] > h - 7.0;
        self.corners.iter().filter(|c| near(**c)).count() >= 3
    }

    pub fn new(photo: RgbImage) -> Self {
        let corners = vision::detect_paper(&photo).unwrap_or_else(|| {
            let (w, h) = (photo.width() as f64, photo.height() as f64);
            vision::order_corners([[w * 0.1, h * 0.1], [w * 0.9, h * 0.1], [w * 0.9, h * 0.9], [w * 0.1, h * 0.9]])
        });
        Self {
            photo,
            corners,
            excluded_pts: Vec::new(),
            notches: Vec::new(),
            depth_pts: Vec::new(),
            rectified: None,
            seg: None,
            excluded_labels: Vec::new(),
            outline: None,
            plate: None,
            keys: Default::default(),
        }
    }

    /// Met à jour les étapes nécessaires. Retourne vrai si quelque chose a changé.
    pub fn update(&mut self, s: &Settings) -> bool {
        let mut changed = false;
        let k0 = format!("{:?}{:?}{}", self.corners, s.paper, s.ppm);
        if k0 != self.keys[0] || self.rectified.is_none() {
            self.rectified = Some(vision::rectify(&self.photo, &self.corners, s.paper.long, s.paper.short, s.ppm));
            self.keys[0] = k0.clone();
            changed = true;
        }
        let k1 = format!("{k0}{:?}", s.seg);
        if k1 != self.keys[1] || changed {
            self.seg = Some(vision::segment(self.rectified.as_ref().unwrap(), s.ppm, &s.seg));
            self.keys[1] = k1.clone();
            changed = true;
        }
        let k2 = format!(
            "{k1}{}{}{}{}{:?}{:?}{:?}{}",
            s.tol_mm, s.simplify_mm, s.smooth, s.notch_d, self.excluded_pts, self.notches, self.depth_pts, s.plate.depth
        );
        if k2 != self.keys[2] || changed {
            let seg = self.seg.as_ref().unwrap();
            self.excluded_labels = self
                .excluded_pts
                .iter()
                .filter_map(|p| self.label_at(*p, s.ppm))
                .collect();
            let mask = vision::pocket_mask(seg, &self.excluded_labels, s.ppm, s.tol_mm, &self.notches, s.notch_d);
            let mut o = vision::extract_outline(&mask, seg.w, seg.h, s.ppm, s.simplify_mm, s.smooth);
            self.assign_depths(&mut o, s);
            self.outline = Some(o);
            self.keys[2] = k2.clone();
            changed = true;
        }
        let k3 = format!("{k2}{:?}", s.plate);
        if k3 != self.keys[3] || changed {
            self.plate = Some(build_plate(self.outline.as_ref().unwrap(), &s.plate));
            self.keys[3] = k3;
            changed = true;
        }
        changed
    }

    /// Profondeur propre de l'objet portant cette étiquette, s'il en a une.
    pub fn custom_depth(&self, label: u32, ppm: f64) -> Option<f64> {
        self.depth_pts.iter().find(|(q, _)| self.label_at(*q, ppm) == Some(label)).map(|e| e.1)
    }

    /// Fixe (Some) ou retire (None) la profondeur propre d'un objet.
    pub fn set_depth(&mut self, label: u32, at: P2, d: Option<f64>, ppm: f64) {
        let labels: Vec<Option<u32>> = self.depth_pts.iter().map(|(q, _)| self.label_at(*q, ppm)).collect();
        let mut it = labels.into_iter();
        self.depth_pts.retain(|_| it.next().flatten() != Some(label));
        if let Some(d) = d {
            self.depth_pts.push((at, d));
        }
    }

    /// Reporte les profondeurs des objets sur les empreintes qui les contiennent.
    /// Une empreinte regroupant plusieurs objets prend la plus grande profondeur.
    fn assign_depths(&self, o: &mut Outline, s: &Settings) {
        if self.depth_pts.is_empty() {
            return;
        }
        let seg = self.seg.as_ref().unwrap();
        let areas: Vec<f64> = o.rings.iter().map(|r| vision::poly_area(r).abs()).collect();
        for obj in &seg.objects {
            if self.excluded_labels.contains(&obj.label) {
                continue;
            }
            let d = self.custom_depth(obj.label, s.ppm).unwrap_or(s.plate.depth);
            let q = [obj.sample_mm[0], o.sheet_h_mm - obj.sample_mm[1]];
            let ring = (0..o.rings.len())
                .filter(|&i| !o.is_hole[i] && crate::mesh::point_in(q, &o.rings[i]))
                .min_by(|&a, &b| areas[a].partial_cmp(&areas[b]).unwrap());
            if let Some(i) = ring {
                o.depth[i] = Some(o.depth[i].map_or(d, |b: f64| b.max(d)));
            }
        }
    }

    /// Étiquette de l'objet situé au point (mm, repère feuille), en tolérant 3 mm autour.
    pub fn label_at(&self, p: P2, ppm: f64) -> Option<u32> {
        let seg = self.seg.as_ref()?;
        let (cx, cy) = (p[0] * ppm, p[1] * ppm);
        let r = (3.0 * ppm) as i64;
        let mut best: Option<(i64, u32)> = None;
        for dy in -r..=r {
            for dx in -r..=r {
                let (x, y) = (cx as i64 + dx, cy as i64 + dy);
                if x < 0 || y < 0 || x >= seg.w as i64 || y >= seg.h as i64 {
                    continue;
                }
                let l = seg.labels[y as usize * seg.w + x as usize];
                let d = dx * dx + dy * dy;
                if l != 0 && best.is_none_or(|b| d < b.0) {
                    best = Some((d, l));
                }
            }
        }
        best.map(|b| b.1)
    }
}
