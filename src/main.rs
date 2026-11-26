#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! TraceTiroir : à partir d'une photo d'objets posés sur une feuille blanche, génère un plateau
//! imprimable en 3D avec les empreintes des objets (plaque simple ou bac Gridfinity).

mod app;
mod mesh;
mod pipeline;
mod render3d;
mod vision;

use image::{ImageDecoder, ImageReader, RgbImage};
use std::path::Path;

/// Charge une photo en tenant compte de l'orientation EXIF et la limite à 4000 px.
pub fn load_photo(path: &Path) -> Result<RgbImage, String> {
    let mut dec = ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .into_decoder()
        .map_err(|e| e.to_string())?;
    let orient = dec.orientation().map_err(|e| e.to_string())?;
    let mut img = image::DynamicImage::from_decoder(dec).map_err(|e| e.to_string())?;
    img.apply_orientation(orient);
    if img.width().max(img.height()) > 4000 {
        img = img.resize(4000, 4000, image::imageops::FilterType::Triangle);
    }
    Ok(img.to_rgb8())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    #[cfg(windows)]
    if args.len() > 1 {
        // Application fenêtrée : on se rattache à la console appelante pour afficher les messages.
        unsafe {
            windows_sys::Win32::System::Console::AttachConsole(windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS);
        }
    }
    if args.len() > 1 && args[1] == "--cli" {
        if let Err(e) = cli(&args[2..]) {
            eprintln!("Erreur : {e}");
            std::process::exit(1);
        }
        return;
    }
    if args.len() > 2 && args[1] == "--demo" {
        demo_photo(Path::new(&args[2]));
        return;
    }
    // Photo passée en argument (« Ouvrir avec… » ou glisser sur l'exe).
    let initial = args.get(1).filter(|a| !a.starts_with("--")).map(std::path::PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1360.0, 860.0])
            .with_title("TraceTiroir — empreintes d'outils pour impression 3D"),
        ..Default::default()
    };
    eframe::run_native("TraceTiroir", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, initial))))).unwrap();
}

/// Mode ligne de commande :
/// tracetiroir --cli photo.jpg sortie.stl [--gridfinity] [--profondeur N] [--jeu N] [--taille LxW] [--objet N=prof] [--papier a4|a3|a5|letter|legal]
fn cli(args: &[String]) -> Result<(), String> {
    if args.len() < 2 {
        return Err("usage : tracetiroir --cli photo.jpg sortie.stl [--gridfinity] [--profondeur N] [--jeu N] [--taille LxW] [--objet N=prof] [--papier a4|a3|a5|letter|legal]".into());
    }
    let mut s = pipeline::Settings::default();
    let mut obj_depths: Vec<(usize, f64)> = Vec::new();
    let mut i = 2;
    while i < args.len() {
        let next = |i: usize| args.get(i + 1).ok_or(format!("valeur manquante après {}", args[i]));
        match args[i].as_str() {
            "--gridfinity" => s.plate.mode = mesh::PlateMode::Gridfinity,
            "--profondeur" => { s.plate.depth = next(i)?.parse().map_err(|_| "profondeur invalide")?; i += 1; }
            "--taille" => {
                let v = next(i)?.to_lowercase();
                let (a, b) = v.split_once('x').ok_or("taille attendue sous la forme 200x120")?;
                s.plate.size = [a.parse().map_err(|_| "taille invalide")?, b.parse().map_err(|_| "taille invalide")?];
                i += 1;
            }
            "--objet" => {
                let v = next(i)?;
                let (n, d) = v.split_once('=').ok_or("--objet attend N=profondeur, ex. 2=25")?;
                obj_depths.push((n.parse().map_err(|_| "numéro d'objet invalide")?, d.parse().map_err(|_| "profondeur invalide")?));
                i += 1;
            }
            "--jeu" => { s.tol_mm = next(i)?.parse().map_err(|_| "jeu invalide")?; i += 1; }
            "--papier" => {
                let v = next(i)?.to_lowercase();
                s.paper = match v.as_str() {
                    "a4" => pipeline::PAPERS[0],
                    "a3" => pipeline::PAPERS[1],
                    "a5" => pipeline::PAPERS[2],
                    "letter" => pipeline::PAPERS[3],
                    "legal" => pipeline::PAPERS[4],
                    _ => return Err(format!("papier inconnu : {v}")),
                };
                i += 1;
            }
            o => return Err(format!("option inconnue : {o}")),
        }
        i += 1;
    }
    let t = std::time::Instant::now();
    let photo = load_photo(Path::new(&args[0]))?;
    let mut p = pipeline::Project::new(photo);
    println!("Coins de la feuille : {:?}", p.corners.map(|c| [c[0].round(), c[1].round()]));
    if p.sheet_cropped() {
        println!("ATTENTION : la feuille semble coupée par le cadrage, l'échelle peut être fausse.");
    }
    p.update(&s);
    for (n, d) in obj_depths {
        let o = p.seg.as_ref().unwrap().objects.get(n.wrapping_sub(1)).cloned().ok_or(format!("objet {n} inexistant"))?;
        p.set_depth(o.label, o.sample_mm, Some(d), s.ppm);
    }
    p.update(&s);
    let seg = p.seg.as_ref().unwrap();
    println!("{} objet(s) détecté(s) :", seg.objects.len());
    for (k, o) in seg.objects.iter().enumerate() {
        print!("  {}. ", k + 1);
        println!("centre ({:.0}, {:.0}) mm, surface {:.0} mm²", o.center_mm[0], o.center_mm[1], o.area_mm2);
    }
    let plate = p.plate.as_ref().unwrap().as_ref().map_err(|e| e.clone())?;
    let out = Path::new(&args[1]);
    mesh::write_stl(out, &plate.tris).map_err(|e| e.to_string())?;
    mesh::write_svg(&out.with_extension("svg"), plate).map_err(|e| e.to_string())?;
    // Image de contrôle : feuille redressée + contours.
    let mut dbg = p.rectified.clone().unwrap();
    let o = p.outline.as_ref().unwrap();
    for r in &o.rings {
        for k in 0..r.len() {
            let a = r[k];
            let b = r[(k + 1) % r.len()];
            let f = |q: vision::P2| ((q[0] * s.ppm) as f32, ((o.sheet_h_mm - q[1]) * s.ppm) as f32);
            imageproc::drawing::draw_line_segment_mut(&mut dbg, f(a), f(b), image::Rgb([255, 0, 0]));
        }
    }
    dbg.save(out.with_extension("controle.png")).map_err(|e| e.to_string())?;
    let px = render3d::render(&plate.tris, 1200, 800, -0.6, 0.9, false);
    image::RgbaImage::from_raw(1200, 800, px)
        .unwrap()
        .save(out.with_extension("apercu.png"))
        .map_err(|e| e.to_string())?;
    println!("{} — {} triangles — {} ms", plate.info, plate.tris.len(), t.elapsed().as_millis());
    println!("Écrit : {} (+ .svg, .controle.png)", out.display());
    Ok(())
}

/// Génère une photo de test synthétique : feuille A4 en perspective avec quelques objets.
fn demo_photo(path: &Path) {
    use imageproc::drawing::{draw_filled_circle_mut, draw_filled_rect_mut, draw_polygon_mut};
    use imageproc::point::Point;
    use imageproc::rect::Rect;
    let k = 6.0f32; // px/mm sur la feuille
    let (sw, sh) = ((297.0 * k) as u32, (210.0 * k) as u32);
    let mut sheet = RgbImage::from_pixel(sw, sh, image::Rgb([245, 243, 238]));
    let px = |v: f32| (v * k) as i32;
    // Clé plate : manche + deux têtes rondes, avec un trou.
    draw_filled_rect_mut(&mut sheet, Rect::at(px(40.0), px(40.0)).of_size((150.0 * k) as u32, (14.0 * k) as u32), image::Rgb([60, 60, 65]));
    draw_filled_circle_mut(&mut sheet, (px(40.0), px(47.0)), px(13.0), image::Rgb([60, 60, 65]));
    draw_filled_circle_mut(&mut sheet, (px(190.0), px(47.0)), px(11.0), image::Rgb([60, 60, 65]));
    draw_filled_circle_mut(&mut sheet, (px(40.0), px(47.0)), px(5.0), image::Rgb([245, 243, 238]));
    // Tournevis : tige grise + manche rouge.
    draw_filled_rect_mut(&mut sheet, Rect::at(px(60.0), px(120.0)).of_size((110.0 * k) as u32, (6.0 * k) as u32), image::Rgb([110, 110, 115]));
    draw_filled_rect_mut(&mut sheet, Rect::at(px(170.0), px(113.0)).of_size((90.0 * k) as u32, (20.0 * k) as u32), image::Rgb([200, 40, 40]));
    // Triangle jaune (objet clair coloré).
    draw_polygon_mut(&mut sheet, &[Point::new(px(220.0), px(30.0)), Point::new(px(270.0), px(30.0)), Point::new(px(245.0), px(90.0))], image::Rgb([235, 200, 40]));
    // Pièce de monnaie.
    draw_filled_circle_mut(&mut sheet, (px(100.0), px(170.0)), px(12.0), image::Rgb([150, 120, 70]));

    let (w, h) = (3000u32, 2200u32);
    let mut photo = RgbImage::from_fn(w, h, |x, y| {
        let n = ((x.wrapping_mul(7919) ^ y.wrapping_mul(104729)) % 23) as u8;
        image::Rgb([70 + n, 55 + n, 45 + n])
    });
    let to = [(420.0, 380.0), (2620.0, 300.0), (2780.0, 1900.0), (300.0, 1960.0)];
    let from = [(0.0, 0.0), (sw as f32, 0.0), (sw as f32, sh as f32), (0.0, sh as f32)];
    let proj = imageproc::geometric_transformations::Projection::from_control_points(from, to).unwrap();
    let inv = proj.invert();
    for y in 0..h {
        for x in 0..w {
            let (u, v) = inv * (x as f32, y as f32);
            if u >= 0.0 && v >= 0.0 && u < sw as f32 - 1.0 && v < sh as f32 - 1.0 {
                // léger dégradé d'éclairage
                let shade = 0.85 + 0.15 * (u / sw as f32);
                let c = sheet.get_pixel(u as u32, v as u32);
                photo.put_pixel(x, y, image::Rgb([(c[0] as f32 * shade) as u8, (c[1] as f32 * shade) as u8, (c[2] as f32 * shade) as u8]));
            }
        }
    }
    photo.save(path).unwrap();
    println!("Photo de test écrite : {}", path.display());
}
