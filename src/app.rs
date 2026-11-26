//! Interface graphique (egui).

use crate::mesh::{self, PlateMode};
use crate::pipeline::{PAPERS, Project, Settings};
use crate::render3d;
use crate::vision::P2;
use eframe::egui::{
    self, Color32, ColorImage, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Stroke, TextStyle, TextureHandle,
    TextureOptions, Vec2,
};
use std::time::{Duration, Instant};

const CUSTOM: &str = "Personnalisé";

// ---------------------------------------------------------------------------------------------
// Palette « atelier » : graphite + orange filament.
// ---------------------------------------------------------------------------------------------
const BG: Color32 = Color32::from_rgb(22, 24, 28);
const PANEL: Color32 = Color32::from_rgb(30, 33, 38);
const CARD: Color32 = Color32::from_rgb(38, 42, 49);
const CARD_STROKE: Color32 = Color32::from_rgb(54, 59, 68);
const WIDGET: Color32 = Color32::from_rgb(50, 55, 64);
const WIDGET_HOVER: Color32 = Color32::from_rgb(62, 68, 79);
const TEXT: Color32 = Color32::from_rgb(228, 231, 236);
const MUTED: Color32 = Color32::from_rgb(146, 153, 165);
const ACCENT: Color32 = Color32::from_rgb(240, 152, 56);
const ACCENT_DARK: Color32 = Color32::from_rgb(176, 104, 30);
const ON_ACCENT: Color32 = Color32::from_rgb(28, 22, 16);
const OK: Color32 = Color32::from_rgb(110, 205, 150);
const ERR: Color32 = Color32::from_rgb(240, 100, 95);
const WARN_BG: Color32 = Color32::from_rgb(64, 48, 26);

// ---------------------------------------------------------------------------------------------
// Textes des infobulles.
// ---------------------------------------------------------------------------------------------
const TIP_PAPER: &str = "Format réel de la feuille posée sous les objets. Elle sert d'étalon : \
une erreur de format fausse toutes les cotes du plateau.";
const TIP_CUSTOM: &str = "Dimension réelle, en mm, de la zone délimitée par les 4 poignées \
(onglet Photo). Utile si la feuille est coupée sur la photo ou si vous utilisez un autre support.";
const TIP_EDGES: &str = "Recommandé. Repère les objets à leurs contours francs : les ombres, \
dont le bord est flou, sont ignorées.\n\nDécochez pour un simple seuil de luminosité : plus simple, \
mais les ombres sont incluses dans les empreintes.";
const TIP_EDGE_THR: &str = "Contraste minimal pour qu'un bord soit retenu.\n\n• Plus bas : détecte \
les objets peu contrastés (transparents, gris clair).\n• Plus haut : ignore les reflets et le grain \
du papier.";
const TIP_SEED: &str = "Les pixels plus sombres que cette fraction du blanc du papier comptent \
d'office comme objet, même sans bord net. Aide pour les objets noirs mats.\n\nTrop haut, les ombres \
franches reviennent.";
const TIP_DARK: &str = "Un pixel plus sombre que cette fraction du blanc du papier compte comme \
objet.\n\nPlus haut : détecte les objets clairs, mais aussi les ombres.";
const TIP_SAT: &str = "Saturation à partir de laquelle un pixel coloré compte comme objet, même \
s'il est clair (plastique jaune, manche rouge…).\n\nPlus bas = plus sensible, mais un papier teinté \
par l'éclairage peut être pris pour un objet.";
const TIP_MIN_AREA: &str = "Les taches plus petites que cette surface sont ignorées : poussières, \
miettes, reflets.";
const TIP_MARGIN: &str = "Bande ignorée le long du bord de la feuille, pour que la table ou \
l'ombre du bord ne soient pas prises pour un objet.";
const TIP_CLOSE: &str = "Referme les petites coupures d'un contour (reflet sur une lame, joint \
entre deux matières).\n\nTrop grand, des objets voisins se soudent.";
const TIP_FILL: &str = "Remplit l'intérieur des objets (anneau de clé, poignée de ciseaux) pour \
une empreinte pleine. Décochez pour garder un plot au centre.";
const TIP_TOL: &str = "Espace ajouté tout autour de chaque objet. 1 mm convient en général ; \
augmentez pour une prise plus facile ou une imprimante peu précise.\n\nDeux objets plus proches que \
2 × le jeu fusionnent en une seule empreinte.";
const TIP_SIMPLIFY: &str = "Écart toléré en simplifiant le contour. Plus haut : fichier plus \
léger, contour plus anguleux.";
const TIP_SMOOTH: &str = "Nombre de passes d'arrondi des angles du contour.\n0 = contour brut, \
3 = très arrondi.";
const TIP_NOTCH: &str = "Diamètre des encoches de préhension : des creux circulaires pour glisser \
un doigt et sortir l'objet. Placez-les par clic dans l'onglet Contours.";
const TIP_SIMPLE: &str = "Plaque rectangulaire aux coins arrondis, à poser au fond d'un tiroir.";
const TIP_GRIDFINITY: &str = "Bac compatible avec le système de rangement modulaire Gridfinity \
(cases de 42 × 42 mm, hauteur par pas de 7 mm), avec ses pieds d'emboîtement. Le rebord \
d'empilement n'est pas généré.";
const TIP_SIZE_MODE: &str = "Décoché : le plateau prend la taille juste nécessaire (empreintes + \
bordure).\n\nCoché : vous imposez la largeur et la longueur, par exemple pour remplir exactement \
un tiroir. Les empreintes sont centrées.";
const TIP_SIZE: &str = "Dimension extérieure du plateau. X suit le grand côté de la feuille, \
Y le petit côté. Elle ne peut pas être inférieure à la taille des empreintes + 2 × la bordure.";
const TIP_GF_MIN: &str = "Taille minimale du bac en cases Gridfinity : X et Y en cases de 42 mm, \
H en unités de 7 mm de hauteur. 0 = calculée automatiquement d'après les objets.";
const TIP_OBJ_DEPTH: &str = "Profondeur propre à cet objet (numéro affiché sur l'image dans \
l'onglet Contours). Un objet épais peut ainsi être plus enfoncé qu'un objet plat.\n\nSi deux objets \
partagent la même empreinte, c'est la plus grande profondeur qui s'applique. Le plateau prend la \
hauteur de l'empreinte la plus profonde.";
const TIP_DEPTH: &str = "Profondeur par défaut des empreintes, pour les objets sans profondeur propre. Visez environ la moitié ou les deux tiers de \
l'épaisseur de l'objet le plus épais, pour pouvoir le saisir.";
const TIP_FLOOR: &str = "Épaisseur de matière sous les empreintes.";
const TIP_BORDER: &str = "Largeur minimale de matière entre les empreintes et le bord extérieur \
du plateau.";
const TIP_RADIUS: &str = "Arrondi des angles extérieurs de la plaque.";
const TIP_STL: &str = "Modèle 3D à ouvrir dans votre trancheur (Cura, PrusaSlicer, Bambu Studio…).";
const TIP_SVG: &str = "Contours vectoriels à l'échelle 1:1 : découpe laser, Inkscape, ou \
impression papier pour vérifier les tailles.";
const TIP_DXF: &str = "Contours pour logiciel de CAO ou découpe CNC (mousse, contreplaqué).";
const TIP_NOTCH_MODE: &str = "Clic gauche : ajoute une encoche de préhension à l'endroit cliqué.\n\
Clic droit : retire l'encoche la plus proche.";
const TIP_EXCLUDE_MODE: &str = "Clic sur un objet : l'exclut du plateau (règle, tache prise pour \
un objet…). Cliquez à nouveau pour le réintégrer.";

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Photo,
    Contours,
    Vue3D,
}

#[derive(PartialEq, Clone, Copy)]
enum ClickMode {
    Notch,
    Exclude,
}

pub struct App {
    s: Settings,
    applied: Settings,
    project: Option<Project>,
    file_name: String,
    tab: Tab,
    click: ClickMode,
    photo_tex: Option<TextureHandle>,
    rect_tex: Option<TextureHandle>,
    view_tex: Option<TextureHandle>,
    view_key: String,
    drag_corner: Option<usize>,
    yaw: f32,
    pitch: f32,
    dirty: bool,
    last_change: Instant,
    status: String,
    pending_open: Option<std::path::PathBuf>,
    /// Objet survolé dans la liste des profondeurs (mis en évidence sur l'image).
    hover_obj: Option<u32>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<std::path::PathBuf>) -> Self {
        apply_theme(&cc.egui_ctx);
        let s = Settings::default();
        Self {
            applied: s.clone(),
            s,
            project: None,
            file_name: String::new(),
            tab: Tab::Photo,
            click: ClickMode::Notch,
            photo_tex: None,
            rect_tex: None,
            view_tex: None,
            view_key: String::new(),
            drag_corner: None,
            yaw: -0.6,
            pitch: 0.9,
            dirty: false,
            last_change: Instant::now(),
            status: "Ouvrez une photo des objets posés sur une feuille blanche.".into(),
            pending_open: initial,
            hover_obj: None,
        }
    }

    fn open_photo(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Images", &["jpg", "jpeg", "png", "bmp", "webp", "tif", "tiff"])
            .pick_file()
        else {
            return;
        };
        self.load(ctx, &path);
    }

    fn load(&mut self, ctx: &egui::Context, path: &std::path::Path) {
        match crate::load_photo(path) {
            Ok(img) => {
                let p = Project::new(img);
                let ci = ColorImage::from_rgb([p.photo.width() as usize, p.photo.height() as usize], p.photo.as_raw());
                self.photo_tex = Some(ctx.load_texture("photo", ci, TextureOptions::LINEAR));
                self.project = Some(p);
                self.file_name = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
                self.process(ctx);
                self.tab = Tab::Contours;
            }
            Err(e) => self.status = format!("Erreur d'ouverture : {e}"),
        }
    }

    fn process(&mut self, ctx: &egui::Context) {
        let Some(p) = self.project.as_mut() else { return };
        let t = Instant::now();
        if p.update(&self.s) {
            self.rect_tex = Some(ctx.load_texture("rect", overlay_image(p), TextureOptions::LINEAR));
            self.view_key.clear();
        }
        self.applied = self.s.clone();
        self.dirty = false;
        let n = p.seg.as_ref().map(|s| s.objects.len()).unwrap_or(0);
        let excl = p.excluded_labels.len();
        self.status = match p.plate.as_ref() {
            Some(Ok(pl)) => format!(
                "{} objet(s) détecté(s){}  ·  {}  ·  {} triangles  ·  {} ms",
                n,
                if excl > 0 { format!(", {excl} ignoré(s)") } else { String::new() },
                pl.info,
                pl.tris.len(),
                t.elapsed().as_millis()
            ),
            Some(Err(e)) => format!("{n} objet(s)  ·  {e}"),
            None => String::new(),
        };
    }

    fn side_panel(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.add_space(4.0);
        ui.label(RichText::new("TraceTiroir").size(24.0).strong().color(TEXT));
        ui.label(RichText::new("Empreintes d'outils à imprimer en 3D").color(MUTED));
        ui.add_space(10.0);
        let w = ui.available_width();
        if ui
            .add(primary_button("📂  Ouvrir une photo…", Vec2::new(w, 36.0)))
            .on_hover_text(
                "Photo prise du dessus, objets posés sur une feuille blanche. \
                 Vous pouvez aussi glisser-déposer l'image dans la fenêtre.",
            )
            .clicked()
        {
            self.open_photo(&ctx);
        }
        if !self.file_name.is_empty() {
            ui.label(RichText::new(&self.file_name).small().color(MUTED));
        }
        ui.add_space(12.0);

        let cropped = self.project.as_ref().is_some_and(|p| p.sheet_cropped());
        let has_project = self.project.is_some();
        card(ui, "1", "Feuille de référence", |ui| {
            grid(ui, "g_paper", |ui| {
                row(ui, "Format", TIP_PAPER, |ui| {
                    egui::ComboBox::from_id_salt("paper")
                        .width(ui.spacing().slider_width + 60.0)
                        .selected_text(self.s.paper.name)
                        .show_ui(ui, |ui| {
                            for p in PAPERS {
                                ui.selectable_value(&mut self.s.paper, p, p.name);
                            }
                            if ui.selectable_label(self.s.paper.name == CUSTOM, CUSTOM).clicked() {
                                self.s.paper.name = CUSTOM;
                            }
                        })
                        .response
                });
                if self.s.paper.name == CUSTOM {
                    row(ui, "Grand côté", TIP_CUSTOM, |ui| {
                        ui.add(egui::DragValue::new(&mut self.s.paper.long).range(20.0..=2000.0).speed(0.5).suffix(" mm"))
                    });
                    row(ui, "Petit côté", TIP_CUSTOM, |ui| {
                        ui.add(egui::DragValue::new(&mut self.s.paper.short).range(20.0..=2000.0).speed(0.5).suffix(" mm"))
                    });
                }
            });
            if cropped {
                callout(
                    ui,
                    "⚠  La feuille semble coupée par le cadrage : l'échelle risque d'être fausse. Reprenez la photo \
                     avec un peu de table visible tout autour, ou choisissez « Personnalisé » avec les dimensions \
                     de la zone visible.",
                );
            }
            if has_project
                && ui
                    .button("⟲  Re-détecter les coins")
                    .on_hover_text("Relance la recherche automatique de la feuille, par exemple après avoir déplacé une poignée par erreur.")
                    .clicked()
            {
                let p = self.project.as_mut().unwrap();
                if let Some(c) = crate::vision::detect_paper(&p.photo) {
                    p.corners = c;
                    self.dirty = true;
                }
            }
        });

        card(ui, "2", "Détection des objets", |ui| {
            ui.checkbox(&mut self.s.seg.edges, "Par bords nets (ignore les ombres)")
                .on_hover_ui(|ui| tip_ui(ui, "Détection par bords nets", TIP_EDGES));
            ui.add_space(4.0);
            grid(ui, "g_seg", |ui| {
                if self.s.seg.edges {
                    row(ui, "Force des bords", TIP_EDGE_THR, |ui| {
                        ui.add(egui::Slider::new(&mut self.s.seg.edge_thr, 15.0..=200.0).max_decimals(0))
                    });
                    row(ui, "Zones très sombres", TIP_SEED, |ui| ui.add(egui::Slider::new(&mut self.s.seg.seed_dark, 0.1..=0.9)));
                } else {
                    row(ui, "Seuil sombre", TIP_DARK, |ui| ui.add(egui::Slider::new(&mut self.s.seg.dark, 0.40..=0.97)));
                }
                row(ui, "Seuil couleur", TIP_SAT, |ui| ui.add(egui::Slider::new(&mut self.s.seg.sat, 0.05..=1.0)));
                row(ui, "Surface minimale", TIP_MIN_AREA, |ui| {
                    ui.add(
                        egui::Slider::new(&mut self.s.seg.min_area_mm2, 5.0..=2000.0)
                            .logarithmic(true)
                            .max_decimals(0)
                            .suffix(" mm²"),
                    )
                });
                row(ui, "Marge au bord", TIP_MARGIN, |ui| {
                    ui.add(egui::Slider::new(&mut self.s.seg.margin_mm, 0.0..=20.0).suffix(" mm"))
                });
                row(ui, "Recollage", TIP_CLOSE, |ui| ui.add(egui::Slider::new(&mut self.s.seg.close_mm, 0.0..=5.0).suffix(" mm")));
            });
            ui.checkbox(&mut self.s.seg.fill_holes, "Boucher les trous des objets")
                .on_hover_ui(|ui| tip_ui(ui, "Boucher les trous", TIP_FILL));
        });

        card(ui, "3", "Empreintes", |ui| {
            grid(ui, "g_pock", |ui| {
                row(ui, "Jeu autour", TIP_TOL, |ui| {
                    ui.add(egui::Slider::new(&mut self.s.tol_mm, 0.0..=5.0).step_by(0.1).suffix(" mm"))
                });
                row(ui, "Simplification", TIP_SIMPLIFY, |ui| {
                    ui.add(egui::Slider::new(&mut self.s.simplify_mm, 0.05..=1.0).suffix(" mm"))
                });
                row(ui, "Lissage", TIP_SMOOTH, |ui| ui.add(egui::Slider::new(&mut self.s.smooth, 0..=3)));
                row(ui, "Ø encoche doigt", TIP_NOTCH, |ui| {
                    ui.add(egui::Slider::new(&mut self.s.notch_d, 8.0..=40.0).max_decimals(0).suffix(" mm"))
                });
            });
            ui.label(RichText::new("Encoches et objets ignorés se placent dans l'onglet Contours.").small().color(MUTED));
        });

        let cur_size = self
            .project
            .as_ref()
            .and_then(|p| p.plate.as_ref())
            .and_then(|r| r.as_ref().ok())
            .map(|pl| pl.size);
        card(ui, "4", "Plateau", |ui| {
            segmented(
                ui,
                &mut self.s.plate.mode,
                &[(PlateMode::Simple, "Plaque simple", TIP_SIMPLE), (PlateMode::Gridfinity, "Gridfinity", TIP_GRIDFINITY)],
            );
            ui.add_space(6.0);
            grid(ui, "g_plate", |ui| {
                if self.s.plate.mode == PlateMode::Simple {
                    let auto = self.s.plate.size[0] <= 0.0 && self.s.plate.size[1] <= 0.0;
                    row(ui, "Dimensions", TIP_SIZE_MODE, |ui| {
                        let mut fixed = !auto;
                        let r = ui.checkbox(&mut fixed, "imposées");
                        if fixed && auto {
                            // On part de la taille automatique actuelle, arrondie au mm supérieur.
                            let c = cur_size.unwrap_or([100.0, 100.0, 0.0]);
                            self.s.plate.size = [c[0].ceil(), c[1].ceil()];
                        } else if !fixed && !auto {
                            self.s.plate.size = [0.0, 0.0];
                        }
                        r
                    });
                    if !auto {
                        row(ui, "Largeur (X)", TIP_SIZE, |ui| {
                            ui.add(egui::DragValue::new(&mut self.s.plate.size[0]).range(10.0..=1000.0).speed(0.5).suffix(" mm"))
                        });
                        row(ui, "Longueur (Y)", TIP_SIZE, |ui| {
                            ui.add(egui::DragValue::new(&mut self.s.plate.size[1]).range(10.0..=1000.0).speed(0.5).suffix(" mm"))
                        });
                    }
                } else {
                    row(ui, "Taille min. (cases)", TIP_GF_MIN, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            for (k, l) in ["X", "Y", "H"].iter().enumerate() {
                                ui.add(egui::DragValue::new(&mut self.s.plate.gf_min[k]).range(0..=12).prefix(format!("{l} ")));
                            }
                        })
                        .response
                    });
                }
                row(ui, "Profondeur défaut", TIP_DEPTH, |ui| ui.add(egui::Slider::new(&mut self.s.plate.depth, 2.0..=80.0).suffix(" mm")));
                row(ui, "Fond", TIP_FLOOR, |ui| ui.add(egui::Slider::new(&mut self.s.plate.floor, 0.6..=10.0).suffix(" mm")));
                row(ui, "Bordure", TIP_BORDER, |ui| ui.add(egui::Slider::new(&mut self.s.plate.margin, 1.0..=30.0).suffix(" mm")));
                if self.s.plate.mode == PlateMode::Simple {
                    row(ui, "Rayon des coins", TIP_RADIUS, |ui| {
                        ui.add(egui::Slider::new(&mut self.s.plate.corner_r, 0.0..=20.0).suffix(" mm"))
                    });
                }
            });
            self.depth_list(ui);
            if let Some(Some(r)) = self.project.as_ref().map(|p| p.plate.as_ref()) {
                ui.add_space(4.0);
                match r {
                    Ok(pl) => ui.label(RichText::new(format!("Résultat : {}", pl.info)).color(OK)),
                    Err(e) => ui.label(RichText::new(format!("⚠  {e}")).color(ERR)),
                };
            }
        });

        card(ui, "5", "Exporter", |ui| {
            let plate_ok = matches!(self.project.as_ref().and_then(|p| p.plate.as_ref()), Some(Ok(_)));
            let enabled = plate_ok && !self.dirty;
            let bw = (ui.available_width() - 2.0 * ui.spacing().item_spacing.x) / 3.0;
            ui.horizontal(|ui| {
                for (label, ext, tip) in [("STL", "stl", TIP_STL), ("SVG", "svg", TIP_SVG), ("DXF", "dxf", TIP_DXF)] {
                    let b = ui
                        .add_enabled(enabled, egui::Button::new(RichText::new(label).strong()).min_size(Vec2::new(bw, 32.0)))
                        .on_hover_text(tip)
                        .on_disabled_hover_text("Ouvrez une photo et attendez la fin du calcul.");
                    if b.clicked() {
                        self.export(ext);
                    }
                }
            });
        });

        egui::CollapsingHeader::new(RichText::new("💡  Conseils de prise de vue").color(MUTED)).show(ui, tips_ui);
        ui.add_space(8.0);
    }

    /// Liste des objets avec leur profondeur, modifiable une par une.
    fn depth_list(&mut self, ui: &mut egui::Ui) {
        let ppm = self.applied.ppm;
        let default = self.s.plate.depth;
        let Some(p) = self.project.as_mut() else { return };
        let Some(seg) = p.seg.as_ref() else { return };
        let objs: Vec<(usize, crate::vision::ObjInfo)> = seg
            .objects
            .iter()
            .cloned()
            .enumerate()
            .filter(|(_, o)| !p.excluded_labels.contains(&o.label))
            .collect();
        if objs.is_empty() {
            return;
        }
        ui.add_space(6.0);
        ui.label(RichText::new("Profondeur par objet").strong().color(TEXT))
            .on_hover_ui(|ui| tip_ui(ui, "Profondeur par objet", TIP_OBJ_DEPTH));
        let mut changed = false;
        grid(ui, "g_depths", |ui| {
            for (k, o) in &objs {
                let custom = p.custom_depth(o.label, ppm);
                let hot = self.hover_obj == Some(o.label);
                let l = ui
                    .horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        badge(ui, &format!("{}", k + 1), hot || custom.is_some());
                        ui.label(RichText::new(format!("{:.0} cm²", o.area_mm2 / 100.0)).small().color(MUTED));
                    })
                    .response
                    .interact(Sense::hover());
                let mut d = custom.unwrap_or(default);
                let r = ui
                    .horizontal(|ui| {
                        let mut txt = egui::DragValue::new(&mut d).range(1.0..=80.0).speed(0.2).max_decimals(1).suffix(" mm");
                        if custom.is_none() {
                            txt = txt.prefix("défaut ");
                        }
                        let dv = ui.add(txt);
                        if dv.changed() {
                            p.set_depth(o.label, o.sample_mm, Some(d), ppm);
                            changed = true;
                        }
                        if custom.is_some()
                            && ui.small_button("⟲").on_hover_text("Revenir à la profondeur par défaut.").clicked()
                        {
                            p.set_depth(o.label, o.sample_mm, None, ppm);
                            changed = true;
                        }
                        dv
                    })
                    .inner;
                if l.hovered() || r.hovered() {
                    self.hover_obj = Some(o.label);
                }
                l.on_hover_ui(|ui| tip_ui(ui, &format!("Objet {}", k + 1), TIP_OBJ_DEPTH));
                r.on_hover_ui(|ui| tip_ui(ui, &format!("Objet {}", k + 1), TIP_OBJ_DEPTH));
                ui.end_row();
            }
        });
        if changed {
            self.dirty = true;
            self.last_change = Instant::now();
        }
    }

    fn export(&mut self, ext: &str) {
        let Some(Some(Ok(plate))) = self.project.as_ref().map(|p| p.plate.as_ref()) else { return };
        let stem = std::path::Path::new(&self.file_name)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "plateau".into());
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(format!("{stem}_empreintes.{ext}"))
            .add_filter(ext.to_uppercase(), &[ext])
            .save_file()
        else {
            return;
        };
        let r = match ext {
            "stl" => mesh::write_stl(&path, &plate.tris),
            "svg" => mesh::write_svg(&path, plate),
            _ => mesh::write_dxf(&path, plate),
        };
        self.status = match r {
            Ok(()) => format!("Exporté : {}", path.display()),
            Err(e) => format!("Erreur d'export : {e}"),
        };
    }

    fn photo_view(&mut self, ui: &mut egui::Ui) {
        let (Some(p), Some(tex)) = (self.project.as_mut(), self.photo_tex.as_ref()) else { return };
        hint(ui, "Faites glisser les 4 poignées sur les coins de la feuille. Le calcul se relance au relâchement.");
        let (iw, ih) = (p.photo.width() as f32, p.photo.height() as f32);
        let (rect, resp) = fit_rect(ui, iw, ih, Sense::click_and_drag());
        let sc = rect.width() / iw;
        let to_screen = |q: P2| Pos2::new(rect.min.x + q[0] as f32 * sc, rect.min.y + q[1] as f32 * sc);
        let painter = ui.painter_at(rect);
        painter.image(tex.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        if resp.drag_started() {
            if let Some(pos) = resp.interact_pointer_pos() {
                self.drag_corner = (0..4)
                    .map(|i| (i, to_screen(p.corners[i]).distance(pos)))
                    .filter(|(_, d)| *d < 30.0)
                    .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                    .map(|(i, _)| i);
            }
        }
        if let (Some(i), Some(pos)) = (self.drag_corner, resp.interact_pointer_pos()) {
            if resp.dragged() {
                p.corners[i] = [((pos.x - rect.min.x) / sc) as f64, ((pos.y - rect.min.y) / sc) as f64];
            }
        }
        if resp.drag_stopped() && self.drag_corner.is_some() {
            self.drag_corner = None;
            self.dirty = true;
            self.last_change = Instant::now();
        }
        let hover = resp.hover_pos();
        let pts: Vec<Pos2> = p.corners.iter().map(|c| to_screen(*c)).collect();
        painter.add(egui::Shape::closed_line(pts.clone(), Stroke::new(3.0, Color32::from_black_alpha(120))));
        painter.add(egui::Shape::closed_line(pts.clone(), Stroke::new(1.8, ACCENT)));
        for (i, q) in pts.iter().enumerate() {
            let hot = self.drag_corner == Some(i) || hover.is_some_and(|h| h.distance(*q) < 30.0);
            let r = if hot { 10.0 } else { 7.5 };
            painter.circle_filled(*q, r + 2.0, Color32::from_black_alpha(140));
            painter.circle_filled(*q, r, ACCENT);
            painter.circle_filled(*q, 2.5, ON_ACCENT);
            if i == 0 {
                let t = *q + Vec2::new(14.0, -14.0);
                let galley = painter.layout_no_wrap("grand côté →".into(), FontId::proportional(13.0), ON_ACCENT);
                let bg = Rect::from_min_size(t - Vec2::new(0.0, galley.size().y), galley.size()).expand2(Vec2::new(6.0, 3.0));
                painter.rect_filled(bg, 4.0, ACCENT);
                painter.galley(bg.min + Vec2::new(6.0, 3.0), galley, ON_ACCENT);
            }
        }
        if hover.is_some_and(|h| pts.iter().any(|q| h.distance(*q) < 30.0)) || self.drag_corner.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
    }

    fn contour_view(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Clic sur l'image :").color(MUTED));
            segmented(
                ui,
                &mut self.click,
                &[(ClickMode::Notch, "◯  Ajouter une encoche", TIP_NOTCH_MODE), (ClickMode::Exclude, "⊘  Ignorer un objet", TIP_EXCLUDE_MODE)],
            );
            ui.add_space(12.0);
            legend(ui, Color32::from_rgb(70, 130, 240), "objet");
            legend(ui, Color32::from_rgb(230, 70, 70), "ignoré");
            legend(ui, ACCENT_DARK, "contour");
            legend(ui, Color32::from_rgb(60, 160, 255), "encoche");
        });
        ui.label(
            RichText::new(match self.click {
                ClickMode::Notch => "Cliquez au bord d'une empreinte pour y ajouter une encoche ; clic droit pour retirer la plus proche.",
                ClickMode::Exclude => "Cliquez sur un objet pour l'ignorer ou le réintégrer.",
            })
            .small()
            .color(MUTED),
        );
        ui.add_space(4.0);
        let (Some(p), Some(tex)) = (self.project.as_mut(), self.rect_tex.as_ref()) else { return };
        let ppm = self.applied.ppm;
        let [tw, th] = tex.size();
        let (rect, resp) = fit_rect(ui, tw as f32, th as f32, Sense::click());
        let sc = rect.width() / tw as f32 * ppm as f32; // pixels écran par mm
        let painter = ui.painter_at(rect);
        painter.image(tex.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        let to_screen = |q: P2| Pos2::new(rect.min.x + q[0] as f32 * sc, rect.min.y + q[1] as f32 * sc);
        if let Some(o) = &p.outline {
            for (i, r) in o.rings.iter().enumerate() {
                let pts: Vec<Pos2> = r.iter().map(|q| to_screen([q[0], o.sheet_h_mm - q[1]])).collect();
                let col = if o.is_hole[i] { Color32::from_rgb(255, 200, 80) } else { ACCENT_DARK };
                painter.add(egui::Shape::closed_line(pts, Stroke::new(2.0, col)));
            }
        }
        for n in &p.notches {
            let c = to_screen(*n);
            let r = (self.applied.notch_d * 0.5) as f32 * sc;
            painter.circle_filled(c, r, Color32::from_rgba_unmultiplied(60, 160, 255, 40));
            painter.circle_stroke(c, r, Stroke::new(1.8, Color32::from_rgb(60, 160, 255)));
        }
        if let (Some(seg), Some(o)) = (p.seg.as_ref(), p.outline.as_ref()) {
            let _ = o;
            for (k, ob) in seg.objects.iter().enumerate() {
                if p.excluded_labels.contains(&ob.label) {
                    continue;
                }
                let c = to_screen(ob.sample_mm);
                let custom = p.depth_pts.iter().find(|(q, _)| p.label_at(*q, ppm) == Some(ob.label)).map(|e| e.1);
                let hot = self.hover_obj == Some(ob.label);
                let txt = match custom {
                    Some(d) => format!("{}  ·  {:.0} mm", k + 1, d),
                    None => format!("{}", k + 1),
                };
                let fill = if hot || custom.is_some() { ACCENT } else { Color32::from_rgb(40, 44, 52) };
                let fg = if hot || custom.is_some() { ON_ACCENT } else { TEXT };
                let galley = painter.layout_no_wrap(txt, FontId::proportional(if hot { 16.0 } else { 13.0 }), fg);
                let sz = galley.size() + Vec2::new(14.0, 6.0);
                let r = Rect::from_center_size(c, Vec2::new(sz.x.max(sz.y), sz.y));
                painter.rect_filled(r.expand(1.5), sz.y * 0.5 + 1.5, Color32::from_black_alpha(120));
                painter.rect_filled(r, sz.y * 0.5, fill);
                painter.galley(r.center() - galley.size() * 0.5, galley, fg);
            }
        }
        for e in &p.excluded_pts {
            let c = to_screen(*e);
            let d = 7.0;
            painter.circle_filled(c, 12.0, Color32::from_rgb(230, 70, 70));
            painter.line_segment([c + Vec2::new(-d, -d) * 0.7, c + Vec2::new(d, d) * 0.7], Stroke::new(2.5, Color32::WHITE));
            painter.line_segment([c + Vec2::new(-d, d) * 0.7, c + Vec2::new(d, -d) * 0.7], Stroke::new(2.5, Color32::WHITE));
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
            if self.click == ClickMode::Notch {
                if let Some(h) = resp.hover_pos() {
                    painter.circle_stroke(h, (self.applied.notch_d * 0.5) as f32 * sc, Stroke::new(1.0, Color32::from_rgba_unmultiplied(60, 160, 255, 140)));
                }
            }
        }
        let to_mm = |pos: Pos2| -> P2 { [((pos.x - rect.min.x) / sc) as f64, ((pos.y - rect.min.y) / sc) as f64] };
        if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let q = to_mm(pos);
                match self.click {
                    ClickMode::Notch => p.notches.push(q),
                    ClickMode::Exclude => {
                        if let Some(l) = p.label_at(q, ppm) {
                            let before = p.excluded_pts.len();
                            let labels: Vec<Option<u32>> = p.excluded_pts.iter().map(|e| p.label_at(*e, ppm)).collect();
                            let mut it = labels.into_iter();
                            p.excluded_pts.retain(|_| it.next().flatten() != Some(l));
                            if p.excluded_pts.len() == before {
                                p.excluded_pts.push(q);
                            }
                        }
                    }
                }
                self.dirty = true;
                self.last_change = Instant::now() - Duration::from_secs(1);
            }
        }
        if resp.secondary_clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let q = to_mm(pos);
                let near = p
                    .notches
                    .iter()
                    .enumerate()
                    .map(|(i, n)| (i, (n[0] - q[0]).hypot(n[1] - q[1])))
                    .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
                if let Some((i, _)) = near {
                    p.notches.remove(i);
                    self.dirty = true;
                    self.last_change = Instant::now() - Duration::from_secs(1);
                }
            }
        }
    }

    fn view3d(&mut self, ui: &mut egui::Ui) {
        let Some(p) = self.project.as_ref() else { return };
        let plate = match p.plate.as_ref() {
            Some(Ok(pl)) => pl,
            Some(Err(e)) => {
                callout(ui, &format!("⚠  {e}"));
                return;
            }
            None => return,
        };
        ui.horizontal(|ui| {
            hint(ui, "Faites glisser pour tourner la vue.");
            if ui.small_button("⟲ Vue par défaut").on_hover_text("Revenir à l'angle de vue initial.").clicked() {
                self.yaw = -0.6;
                self.pitch = 0.9;
            }
        });
        let avail = ui.available_size();
        let (w, h) = ((avail.x as usize).clamp(64, 1400), (avail.y as usize).clamp(64, 1000));
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(w as f32, h as f32), Sense::drag());
        if resp.dragged() {
            let d = resp.drag_delta();
            self.yaw += d.x * 0.01;
            self.pitch = (self.pitch + d.y * 0.01).clamp(-1.5, 1.5);
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(if resp.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
        }
        let key = format!("{w}x{h}{}{}{}", self.yaw, self.pitch, plate.tris.len());
        if key != self.view_key || self.view_tex.is_none() {
            let px = render3d::render(&plate.tris, w, h, self.yaw, self.pitch, true);
            let ci = ColorImage::from_rgba_unmultiplied([w, h], &px);
            self.view_tex = Some(ui.ctx().load_texture("view3d", ci, TextureOptions::LINEAR));
            self.view_key = key;
        }
        if let Some(t) = &self.view_tex {
            ui.painter().image(t.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        let s = plate.size;
        ui.painter().text(
            rect.left_bottom() + Vec2::new(12.0, -12.0),
            egui::Align2::LEFT_BOTTOM,
            format!("{:.1} × {:.1} × {:.1} mm", s[0], s[1], s[2]),
            FontId::proportional(15.0),
            MUTED,
        );
    }

    fn empty_state(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.5 - 220.0).max(20.0));
            egui::Frame::new()
                .fill(PANEL)
                .stroke(Stroke::new(1.5, CARD_STROKE))
                .corner_radius(16)
                .inner_margin(32)
                .show(ui, |ui| {
                    ui.set_width(460.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("📷").size(54.0).color(ACCENT));
                        ui.add_space(6.0);
                        ui.label(RichText::new("Glissez une photo ici").size(22.0).strong().color(TEXT));
                        ui.label(RichText::new("ou").color(MUTED));
                        ui.add_space(4.0);
                        if ui.add(primary_button("📂  Ouvrir une photo…", Vec2::new(240.0, 38.0))).clicked() {
                            self.open_photo(&ctx);
                        }
                        ui.add_space(18.0);
                    });
                    ui.label(RichText::new("Pour une bonne photo").strong().color(TEXT));
                    ui.add_space(2.0);
                    tips_ui(ui);
                });
        });
    }
}

// ---------------------------------------------------------------------------------------------
// Thème et petits composants d'interface.
// ---------------------------------------------------------------------------------------------

fn apply_theme(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.style_mut_of(egui::Theme::Dark, |s| {
        s.text_styles.insert(TextStyle::Body, FontId::proportional(14.5));
        s.text_styles.insert(TextStyle::Button, FontId::proportional(14.5));
        s.text_styles.insert(TextStyle::Small, FontId::proportional(12.0));
        s.text_styles.insert(TextStyle::Heading, FontId::proportional(22.0));
        s.spacing.item_spacing = Vec2::new(8.0, 7.0);
        s.spacing.button_padding = Vec2::new(10.0, 5.0);
        s.spacing.interact_size.y = 24.0;
        s.spacing.slider_width = 130.0;
        s.spacing.tooltip_width = 320.0;
        s.interaction.tooltip_delay = 0.25;

        let v = &mut s.visuals;
        v.panel_fill = PANEL;
        v.window_fill = CARD;
        v.window_stroke = Stroke::new(1.0, CARD_STROKE);
        v.window_corner_radius = CornerRadius::same(10);
        v.menu_corner_radius = CornerRadius::same(8);
        v.extreme_bg_color = BG;
        v.faint_bg_color = CARD;
        v.hyperlink_color = ACCENT;
        v.warn_fg_color = ACCENT;
        v.error_fg_color = ERR;
        v.slider_trailing_fill = true;
        v.selection.bg_fill = ACCENT_DARK;
        v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
        let r = CornerRadius::same(6);
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = CARD;
        w.noninteractive.weak_bg_fill = CARD;
        w.noninteractive.bg_stroke = Stroke::new(1.0, CARD_STROKE);
        w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        w.noninteractive.corner_radius = r;
        w.inactive.bg_fill = WIDGET;
        w.inactive.weak_bg_fill = WIDGET;
        w.inactive.bg_stroke = Stroke::NONE;
        w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
        w.inactive.corner_radius = r;
        w.hovered.bg_fill = WIDGET_HOVER;
        w.hovered.weak_bg_fill = WIDGET_HOVER;
        w.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
        w.hovered.fg_stroke = Stroke::new(1.5, Color32::WHITE);
        w.hovered.corner_radius = r;
        w.active.bg_fill = ACCENT;
        w.active.weak_bg_fill = ACCENT_DARK;
        w.active.bg_stroke = Stroke::new(1.0, ACCENT);
        w.active.fg_stroke = Stroke::new(1.5, Color32::WHITE);
        w.active.corner_radius = r;
        w.open.bg_fill = WIDGET_HOVER;
        w.open.weak_bg_fill = WIDGET_HOVER;
        w.open.bg_stroke = Stroke::new(1.0, ACCENT);
        w.open.fg_stroke = Stroke::new(1.0, TEXT);
        w.open.corner_radius = r;
    });
}

fn primary_button(text: &str, size: Vec2) -> egui::Button<'_> {
    egui::Button::new(RichText::new(text).strong().size(15.0).color(ON_ACCENT))
        .fill(ACCENT)
        .corner_radius(8)
        .min_size(size)
}

/// Carte de section : pastille numérotée + titre + contenu.
fn card(ui: &mut egui::Ui, num: &str, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, CARD_STROKE))
        .corner_radius(10)
        .inner_margin(12)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 11.0, ACCENT);
                ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, num, FontId::proportional(13.0), ON_ACCENT);
                ui.label(RichText::new(title).size(15.5).strong().color(TEXT));
            });
            ui.add_space(6.0);
            add(ui);
        });
    ui.add_space(10.0);
}

/// Pastille numérotée (objet).
fn badge(ui: &mut egui::Ui, text: &str, on: bool) {
    let (r, _) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::hover());
    ui.painter().circle_filled(r.center(), 10.0, if on { ACCENT } else { WIDGET_HOVER });
    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, text, FontId::proportional(12.0), if on { ON_ACCENT } else { TEXT });
}

fn grid(ui: &mut egui::Ui, id: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(id).num_columns(2).spacing([10.0, 8.0]).min_col_width(140.0).show(ui, add);
}

/// Ligne de réglage : libellé (avec pastille « i ») + widget, même infobulle sur les deux.
fn row(ui: &mut egui::Ui, label: &str, tip: &str, add: impl FnOnce(&mut egui::Ui) -> egui::Response) {
    let l = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.label(RichText::new(label).color(TEXT));
            let (r, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
            let c = r.center() + Vec2::new(0.0, 1.0);
            ui.painter().circle_stroke(c, 6.0, Stroke::new(1.0, MUTED));
            ui.painter().text(c, egui::Align2::CENTER_CENTER, "i", FontId::proportional(10.0), MUTED);
        })
        .response
        .interact(Sense::hover());
    l.on_hover_ui(|ui| tip_ui(ui, label, tip));
    add(ui).on_hover_ui(|ui| tip_ui(ui, label, tip));
    ui.end_row();
}

fn tip_ui(ui: &mut egui::Ui, title: &str, body: &str) {
    ui.label(RichText::new(title).strong().color(ACCENT));
    ui.label(body);
}

/// Groupe de boutons exclusifs.
fn segmented<T: PartialEq + Copy>(ui: &mut egui::Ui, value: &mut T, opts: &[(T, &str, &str)]) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (v, label, tip) in opts {
            let sel = *value == *v;
            let text = RichText::new(*label).color(if sel { ON_ACCENT } else { TEXT });
            let b = egui::Button::new(if sel { text.strong() } else { text })
                .fill(if sel { ACCENT } else { WIDGET })
                .corner_radius(6)
                .min_size(Vec2::new(0.0, 28.0));
            if ui.add(b).on_hover_ui(|ui| tip_ui(ui, label, tip)).clicked() {
                *value = *v;
            }
        }
    });
}

fn callout(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new()
        .fill(WARN_BG)
        .stroke(Stroke::new(1.0, ACCENT_DARK))
        .corner_radius(8)
        .inner_margin(10)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(text).color(Color32::from_rgb(250, 210, 160)));
        });
}

fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(MUTED));
}

fn legend(ui: &mut egui::Ui, col: Color32, text: &str) {
    let (r, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
    ui.painter().rect_filled(r, 3.0, col);
    ui.label(RichText::new(text).small().color(MUTED));
}

fn tips_ui(ui: &mut egui::Ui) {
    for t in [
        "Feuille entière visible, posée sur un fond plus sombre.",
        "Photo bien à la verticale, prise d'au moins 1 m (zoom ×2) pour limiter la perspective.",
        "Lumière diffuse : évitez les ombres portées.",
        "Objets à au moins 5 mm du bord de la feuille et non collés entre eux.",
        "Les objets épais paraissent plus gros vus de près : préférez la prise de loin.",
    ] {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("•").color(ACCENT));
            ui.label(RichText::new(t).color(MUTED));
        });
    }
}

/// Image redressée avec les objets détectés teintés (bleu) ou ignorés (rouge).
fn overlay_image(p: &Project) -> ColorImage {
    let img = p.rectified.as_ref().unwrap();
    let seg = p.seg.as_ref().unwrap();
    let mut px = Vec::with_capacity(seg.w * seg.h * 3);
    for (i, c) in img.pixels().enumerate() {
        let l = seg.labels[i];
        let (mut r, mut g, mut b) = (c[0] as f32, c[1] as f32, c[2] as f32);
        if l != 0 {
            let tint = if p.excluded_labels.contains(&l) { [230.0, 70.0, 70.0] } else { [70.0, 130.0, 240.0] };
            r = r * 0.6 + tint[0] * 0.4;
            g = g * 0.6 + tint[1] * 0.4;
            b = b * 0.6 + tint[2] * 0.4;
        }
        px.extend_from_slice(&[r as u8, g as u8, b as u8]);
    }
    ColorImage::from_rgb([seg.w, seg.h], &px)
}

/// Réserve un rectangle à l'échelle de l'image, centré dans l'espace disponible.
fn fit_rect(ui: &mut egui::Ui, iw: f32, ih: f32, sense: Sense) -> (Rect, egui::Response) {
    let avail = ui.available_size();
    let sc = (avail.x / iw).min(avail.y / ih).max(0.01);
    let size = Vec2::new(iw * sc, ih * sc);
    ui.vertical_centered(|ui| ui.allocate_exact_size(size, sense)).inner
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Fichier glissé-déposé
        let dropped: Vec<_> = ctx.input(|i| {
            i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect::<Vec<_>>()
        });
        if let Some(path) = dropped.first() {
            self.load(&ctx, path);
        }
        if let Some(path) = self.pending_open.take() {
            self.load(&ctx, &path);
        }
        let hovering_file = ctx.input(|i| !i.raw.hovered_files.is_empty());
        let prev_hover = self.hover_obj.take();

        egui::Panel::left("params")
            .resizable(true)
            .default_size(390.0)
            .min_size(340.0)
            .frame(egui::Frame::new().fill(PANEL).inner_margin(egui::Margin { left: 14, right: 10, top: 12, bottom: 4 }))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.side_panel(ui));
            });
        egui::Panel::bottom("status")
            .frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin::symmetric(14, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if self.dirty {
                        ui.spinner();
                        ui.label(RichText::new("Calcul…").color(ACCENT));
                    } else if matches!(self.project.as_ref().and_then(|p| p.plate.as_ref()), Some(Ok(_))) {
                        ui.label(RichText::new("●").color(OK));
                    }
                    ui.label(RichText::new(&self.status).color(MUTED));
                });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(14))
            .show(ui, |ui| {
                if self.project.is_none() {
                    self.empty_state(ui);
                } else {
                    segmented(
                        ui,
                        &mut self.tab,
                        &[
                            (Tab::Photo, "①  Photo & feuille", "Vérifiez et ajustez les 4 coins de la feuille."),
                            (Tab::Contours, "②  Contours", "Contrôlez les objets détectés, placez les encoches, ignorez des objets."),
                            (Tab::Vue3D, "③  Aperçu 3D", "Visualisez le plateau qui sera exporté."),
                        ],
                    );
                    ui.add_space(8.0);
                    match self.tab {
                        Tab::Photo => self.photo_view(ui),
                        Tab::Contours => self.contour_view(ui),
                        Tab::Vue3D => self.view3d(ui),
                    }
                }
                if hovering_file {
                    let r = ui.max_rect();
                    ui.painter().rect_filled(r, 12.0, Color32::from_rgba_unmultiplied(240, 152, 56, 40));
                    ui.painter().rect_stroke(r.shrink(4.0), 12.0, Stroke::new(2.5, ACCENT), egui::StrokeKind::Inside);
                    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "Déposez la photo", FontId::proportional(26.0), ACCENT);
                }
            });

        if self.hover_obj != prev_hover {
            ctx.request_repaint();
        }
        // Recalcul différé quand les réglages changent.
        if self.s != self.applied && !self.dirty {
            self.dirty = true;
            self.last_change = Instant::now();
        }
        let pointer_down = ctx.input(|i| i.pointer.any_down());
        if self.dirty && self.project.is_some() {
            if self.last_change.elapsed() > Duration::from_millis(300) && !pointer_down {
                self.process(&ctx);
            } else {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        }
    }
}
