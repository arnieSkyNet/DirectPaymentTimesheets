//! Signature assets are external paths or immutable, owner-specific managed PNGs.
//! Updating a signature never rewrites an already generated payroll PDF.
use eframe::egui;
use image::{DynamicImage, ImageFormat};
use rusqlite::{params, Connection, TransactionBehavior};
use std::{
    io::{Cursor, Read},
    path::{Path, PathBuf},
};
use unicode_normalization::UnicodeNormalization;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_DIMENSION: u32 = 4096;
const WIDTH: u32 = 1000;
const HEIGHT: u32 = 300;

pub fn decode(bytes: &[u8]) -> Result<DynamicImage> {
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        return Err("Signature must be a non-empty PNG or JPEG smaller than 8 MiB".into());
    }
    let format = image::guess_format(bytes)?;
    if !matches!(format, ImageFormat::Png | ImageFormat::Jpeg) {
        return Err("Signature content must be PNG or JPEG (regardless of filename)".into());
    }
    // Require complete containers; decoders can otherwise tolerate truncated JPEGs.
    if format == ImageFormat::Jpeg && !complete_jpeg(bytes) {
        return Err("Signature JPEG is incomplete or has invalid marker lengths".into());
    }
    if format == ImageFormat::Jpeg {
        // image's JPEG adapter deliberately uses permissive decoding. Validate
        // with strict decoding too, before accepting or replacing a reference.
        let options = zune_core::options::DecoderOptions::default()
            .set_strict_mode(true)
            .set_max_width(MAX_DIMENSION as usize)
            .set_max_height(MAX_DIMENSION as usize);
        let mut jpeg = zune_jpeg::JpegDecoder::new_with_options(
            zune_core::bytestream::ZCursor::new(bytes),
            options,
        );
        jpeg.decode()?;
    }
    if format == ImageFormat::Png
        && !bytes.ends_with(&[0, 0, 0, 0, b'I', b'E', b'N', b'D', 174, 66, 96, 130])
    {
        return Err("Signature PNG is incomplete or has trailing data".into());
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    use image::ImageDecoder;
    let orientation = decoder.orientation()?;
    let mut image = DynamicImage::from_decoder(decoder)?;
    if image.width() == 0 || image.height() == 0 {
        return Err("Signature image has no pixels".into());
    }
    image.apply_orientation(orientation);
    Ok(image)
}

// Walk the primary container, skipping length-delimited Exif thumbnails. An
// embedded thumbnail's EOI must not make a truncated primary JPEG look complete.
fn complete_jpeg(bytes: &[u8]) -> bool {
    let mut position = 2;
    let mut scan = false;
    let mut saw_scan = false;
    while position < bytes.len() {
        if scan && bytes[position] != 0xff {
            position += 1;
            continue;
        }
        if bytes[position] != 0xff {
            return false;
        }
        while position < bytes.len() && bytes[position] == 0xff {
            position += 1;
        }
        let Some(&marker) = bytes.get(position) else {
            return false;
        };
        position += 1;
        if scan && (marker == 0 || (0xd0..=0xd7).contains(&marker)) {
            continue;
        }
        scan = false;
        if marker == 0xd9 {
            return saw_scan;
        }
        if marker == 1 {
            continue;
        }
        let Some(length_bytes) = bytes.get(position..position + 2) else {
            return false;
        };
        let length = u16::from_be_bytes([length_bytes[0], length_bytes[1]]) as usize;
        if length < 2 || position + length > bytes.len() {
            return false;
        }
        position += length;
        if marker == 0xda {
            scan = true;
            saw_scan = true;
        }
    }
    false
}

pub fn read(path: &Path) -> Result<Vec<u8>> {
    let path = crate::paths::expand_path(&path.to_path_buf());
    let file = std::fs::File::open(&path)
        .map_err(|e| format!("Cannot read signature {}: {e}", path.display()))?;
    let mut bytes = Vec::new();
    file.take((MAX_BYTES + 1) as u64).read_to_end(&mut bytes)?;
    decode(&bytes).map_err(|e| format!("Invalid signature {}: {e}", path.display()))?;
    Ok(bytes)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Owner {
    Employer(i64),
    Pa(i64),
}
impl Owner {
    fn location(self) -> (&'static str, &'static str, i64) {
        match self {
            Self::Employer(id) => ("employers", "employer_signature", id),
            Self::Pa(id) => ("personal_assistants", "signature", id),
        }
    }
    fn full_name(self, db: &Connection) -> Result<String> {
        let name: String = match self {
            Self::Employer(id) => db.query_row("SELECT COALESCE(name,'') FROM employers WHERE id=?1",[id],|r|r.get(0))?,
            Self::Pa(id) => db.query_row("SELECT trim(COALESCE(first_name,'')||' '||COALESCE(surname,'')) FROM personal_assistants WHERE id=?1",[id],|r|r.get(0))?,
        };
        Ok(if name.trim().is_empty() {
            match self {
                Self::Employer(id) => format!("Employer {id}"),
                Self::Pa(id) => format!("PA {id}"),
            }
        } else {
            name.trim().to_owned()
        })
    }
    fn label(self, db: &Connection) -> Result<String> {
        Ok(match self {
            Self::Employer(id) => format!(
                "Employer {}",
                db.query_row("SELECT name FROM employers WHERE id=?1", [id], |r| r
                    .get::<_, String>(0))?
            ),
            Self::Pa(id) => format!(
                "PA {}",
                db.query_row(
                    "SELECT first_name||' '||surname FROM personal_assistants WHERE id=?1",
                    [id],
                    |r| r.get::<_, String>(0)
                )?
            ),
        })
    }
    pub fn path(self, db: &Connection) -> Result<Option<String>> {
        let (table, column, id) = self.location();
        Ok(db.query_row(
            &format!("SELECT {column} FROM {table} WHERE id=?1"),
            [id],
            |r| r.get(0),
        )?)
    }
}

// Preserve name spelling; only filesystem-forbidden characters are substituted.
fn filename_stem(name: &str) -> String {
    let mut stem: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '-'
            } else {
                c
            }
        })
        .collect();
    stem = stem.trim_end_matches([' ', '.']).to_owned();
    if stem.is_empty() {
        stem = "_".into();
    }
    let device = collision_key(stem.split('.').next().unwrap_or("").trim_end());
    if matches!(
        device.as_str(),
        "con" | "prn" | "aux" | "nul" | "conin$" | "conout$"
    ) || ["com", "lpt"].iter().any(|p| {
        device
            .strip_prefix(p)
            .is_some_and(|n| n.len() == 1 && matches!(n.as_bytes()[0], b'1'..=b'9'))
    }) {
        stem.insert(0, '_');
    }
    // Reserve space for numbering on filesystems with 255-byte components.
    while stem.len() > 180 {
        stem.pop();
    }
    stem.trim_end_matches([' ', '.']).to_owned()
}

// Conservative Unicode caseless/normalisation key for Windows/macOS portability.
// The actual filename retains its Unicode spelling and legitimate punctuation.
fn collision_key(name: &str) -> String {
    name.nfkc()
        .flat_map(char::to_uppercase)
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .nfc()
        .collect()
}
fn next_filename(directory: &Path, stem: &str) -> Result<PathBuf> {
    let mut occupied = std::collections::HashSet::new();
    for entry in std::fs::read_dir(directory)? {
        // Include directories, symlinks and orphan assets; never reclaim names.
        occupied.insert(collision_key(&entry?.file_name().to_string_lossy()));
    }
    for number in 1_u64.. {
        let name = if number == 1 {
            format!("{stem}.png")
        } else {
            format!("{stem} ({number}).png")
        };
        if !occupied.contains(&collision_key(&name)) {
            return Ok(directory.join(name));
        }
    }
    Err("No available signature filename".into())
}

/// Keep maintenance saves from overwriting a signature changed by another
/// instance, and from overlapping production publication, dispatch or restore.
pub fn edit_guard(
    app: &crate::app::Application,
    owner: Option<Owner>,
    expected: Option<&str>,
) -> Result<Option<std::fs::File>> {
    let db = crate::payroll_evidence::open(app)?;
    let guard = crate::timesheet_delivery::production_lock(&db)?;
    if let Some(owner) = owner {
        if owner.path(&db)?.as_deref() != expected {
            return Err("Saved signature changed in another instance. Reload this person before saving; no changes were saved".into());
        }
    }
    Ok(guard)
}

/// Compare-and-swap only the signature field, under recovery/dispatch and writer locks.
/// Human-readable, numbered files are never overwritten, even for identical ink.
fn save_drawn_checked(
    db: &mut Connection,
    root: &Path,
    owner: Owner,
    expected: Option<&str>,
    bytes: &[u8],
    authorised: bool,
    expected_name: &str,
) -> Result<String> {
    if !authorised {
        return Err("Explicit signature authorisation is required".into());
    }
    let image = decode(bytes)?.to_rgba8();
    if image.dimensions() != (WIDTH, HEIGHT)
        || image
            .pixels()
            .any(|p| p[0] != 0 || p[1] != 0 || p[2] != 0 || !matches!(p[3], 0 | 255))
        || !image.pixels().any(|p| p[3] == 255)
    {
        return Err("Drawn signature must contain black ink on a transparent background".into());
    }
    let _lock = crate::timesheet_delivery::production_lock(db)?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if owner.label(&tx)? != expected_name {
        return Err(
            "Signature owner details changed; reopen drawing and review the named person".into(),
        );
    }
    if owner.path(&tx)?.as_deref() != expected {
        return Err("Signature changed in another window; review it before replacing".into());
    }
    let (table, column, id) = owner.location();
    let stem = filename_stem(&owner.full_name(&tx)?);
    let directory = root.join("signatures");
    std::fs::create_dir_all(&directory)?;
    let directory = std::fs::canonicalize(directory)?;
    let mut temporary = tempfile::NamedTempFile::new_in(&directory)?;
    use std::io::Write;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    let path = loop {
        let path = next_filename(&directory, &stem)?;
        match temporary.persist_noclobber(&path) {
            Ok(_) => break path,
            Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => {
                // A writer outside our OS/database lock raced publication.
                // Re-scan, preserving the same verified temporary file.
                temporary = e.file;
            }
            Err(e) => return Err(e.error.into()),
        }
    };
    #[cfg(unix)]
    std::fs::File::open(&directory)?.sync_all()?;
    let reference = path
        .to_str()
        .ok_or("Signature path cannot be represented as text")?
        .to_owned();
    if tx.execute(
        &format!("UPDATE {table} SET {column}=?1 WHERE id=?2"),
        params![reference, id],
    )? != 1
    {
        return Err("Signature owner no longer exists".into());
    }
    tx.commit()?;
    Ok(reference)
}

#[cfg(test)]
pub fn save_drawn(
    db: &mut Connection,
    root: &Path,
    owner: Owner,
    expected: Option<&str>,
    bytes: &[u8],
    authorised: bool,
) -> Result<String> {
    let name = owner.label(db)?;
    save_drawn_checked(db, root, owner, expected, bytes, authorised, &name)
}

#[derive(Default)]
pub struct DrawingUi {
    drawing: Option<Drawing>,
}
struct Drawing {
    owner: Owner,
    name: String,
    expected: Option<String>,
    strokes: Vec<Vec<egui::Pos2>>,
    authorised: bool,
    error: String,
}
impl DrawingUi {
    pub fn open(&mut self, owner: Owner, db: &Connection) -> Result<()> {
        self.drawing = Some(Drawing {
            owner,
            name: owner.label(db)?,
            expected: owner.path(db)?,
            strokes: Vec::new(),
            authorised: false,
            error: String::new(),
        });
        Ok(())
    }
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        app: &crate::app::Application,
    ) -> Option<(Owner, String)> {
        let drawing = self.drawing.as_mut()?;
        let mut use_image = false;
        let mut cancel = false;
        egui::Window::new(format!("Draw signature — {}", drawing.name)).id(egui::Id::new("signature-drawing")).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.label("Draw with a mouse, touchscreen or stylus. Black ink; saved background is transparent.");
            let width = ui.available_width().clamp(250.0,600.0);
            let size = egui::vec2(width,width * HEIGHT as f32 / WIDTH as f32);
            let (response,painter) = ui.allocate_painter(size,egui::Sense::click_and_drag());
            response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other,true,"Signature drawing canvas: use a pointer; import or continue unsigned are alternatives"));
            painter.rect_filled(response.rect,0.0,egui::Color32::WHITE);
            if response.drag_started() || response.clicked() { drawing.strokes.push(Vec::new()); }
            if response.dragged() || response.drag_started() || response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let point = egui::pos2(((pos.x-response.rect.left())/size.x).clamp(0.0,1.0),((pos.y-response.rect.top())/size.y).clamp(0.0,1.0));
                    if let Some(stroke) = drawing.strokes.last_mut() { stroke.push(point); }
                }
            }
            for stroke in &drawing.strokes {
                let points: Vec<_> = stroke.iter().map(|p|response.rect.min+egui::vec2(p.x*size.x,p.y*size.y)).collect();
                if points.len()==1 { painter.circle_filled(points[0],1.5,egui::Color32::BLACK); }
                else if !points.is_empty() { painter.add(egui::Shape::line(points,egui::Stroke::new(3.0_f32,egui::Color32::BLACK))); }
            }
            if ui.button("Clear and redraw").clicked() { drawing.strokes.clear(); drawing.authorised=false; }
            ui.checkbox(&mut drawing.authorised,format!("I am {} or explicitly authorised to record their signature. I authorise replacing their saved signature for future PDFs.",drawing.name));
            ui.horizontal(|ui| {
                use_image=ui.add_enabled(drawing.authorised && drawing.strokes.iter().any(|s|!s.is_empty()),egui::Button::new("Use drawn signature")).clicked();
                cancel=ui.button("Cancel").clicked();
            });
            if !drawing.error.is_empty() { ui.colored_label(egui::Color32::RED,&drawing.error); }
        });
        if use_image {
            let result = (|| -> Result<String> {
                let bytes = rasterize(&drawing.strokes)?;
                let mut db = crate::payroll_evidence::open(app)?;
                save_drawn_checked(
                    &mut db,
                    &app.context.environment.data_dir,
                    drawing.owner,
                    drawing.expected.as_deref(),
                    &bytes,
                    drawing.authorised,
                    &drawing.name,
                )
            })();
            match result {
                Ok(path) => {
                    let owner = drawing.owner;
                    self.drawing = None;
                    return Some((owner, path));
                }
                Err(e) => drawing.error = e.to_string(),
            }
        }
        if cancel {
            self.drawing = None;
        }
        None
    }
}

pub fn rasterize(strokes: &[Vec<egui::Pos2>]) -> Result<Vec<u8>> {
    if strokes.iter().map(Vec::len).sum::<usize>() > 10000
        || strokes.iter().flatten().any(|p| {
            !p.x.is_finite()
                || !p.y.is_finite()
                || !(0.0..=1.0).contains(&p.x)
                || !(0.0..=1.0).contains(&p.y)
        })
    {
        return Err("Drawing is too large or has invalid coordinates; clear and redraw".into());
    }
    if !strokes.iter().any(|s| !s.is_empty()) {
        return Err("Draw a signature before saving".into());
    }
    let mut pixels = image::RgbaImage::new(WIDTH, HEIGHT);
    for stroke in strokes {
        for (index, point) in stroke.iter().enumerate() {
            let previous = if index == 0 {
                point
            } else {
                &stroke[index - 1]
            };
            let a = (
                previous.x * (WIDTH - 1) as f32,
                previous.y * (HEIGHT - 1) as f32,
            );
            let b = (point.x * (WIDTH - 1) as f32, point.y * (HEIGHT - 1) as f32);
            let steps = ((b.0 - a.0).abs().max((b.1 - a.1).abs()).ceil() as u32).max(1);
            for i in 0..=steps {
                let t = i as f32 / steps as f32;
                let x = (a.0 + (b.0 - a.0) * t).round() as i32;
                let y = (a.1 + (b.1 - a.1) * t).round() as i32;
                for dx in -3..=3 {
                    for dy in -3..=3 {
                        if dx * dx + dy * dy <= 9
                            && x + dx >= 0
                            && y + dy >= 0
                            && x + dx < WIDTH as i32
                            && y + dy < HEIGHT as i32
                        {
                            pixels.put_pixel(
                                (x + dx) as u32,
                                (y + dy) as u32,
                                image::Rgba([0, 0, 0, 255]),
                            );
                        }
                    }
                }
            }
        }
    }
    let mut out = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(pixels).write_to(&mut out, ImageFormat::Png)?;
    Ok(out.into_inner())
}

/// Validate without discarding a configured path. None is intentionally unsigned.
pub fn checked_path(reference: Option<&str>, unsigned: bool) -> Result<Option<PathBuf>> {
    if unsigned {
        return Ok(None);
    }
    reference
        .map(|s| {
            let path = crate::paths::expand_path(&PathBuf::from(s));
            read(&path)?;
            Ok(path)
        })
        .transpose()
}

/// Validate first: failure leaves the editor's previous reference untouched.
pub fn select_import(reference: &mut Option<String>, path: &Path) -> Result<()> {
    read(path)?;
    let text = path
        .to_str()
        .ok_or("Signature path cannot be represented as text")?;
    *reference = Some(text.to_owned());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;
    fn ink() -> Vec<u8> {
        rasterize(&[vec![egui::pos2(0.1, 0.4), egui::pos2(0.8, 0.6)]]).unwrap()
    }
    fn jpeg() -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            32,
            12,
            image::Rgb([255, 255, 255]),
        ))
        .write_to(&mut out, ImageFormat::Jpeg)
        .unwrap();
        out.into_inner()
    }
    fn database() -> Connection {
        let db = crate::database::open_in_memory().unwrap();
        crate::database::create_schema(&db).unwrap();
        db.execute_batch("INSERT INTO employers(id,name,employer_signature) VALUES(1,'Employer','previous.png'); INSERT INTO personal_assistants(id,first_name,surname,signature) VALUES(1,'PA','One','pa-old.png'),(2,'PA','Two',NULL);").unwrap();
        db
    }
    #[test]
    fn signature_jfif_exif_progressive_and_grayscale_decode_and_render() {
        let jpeg = jpeg();
        // A valid APP1 Exif little-endian TIFF: orientation 6 (rotate 90 CW).
        let exif = b"Exif\0\0II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0";
        let mut oriented = vec![0xff, 0xd8, 0xff, 0xe1];
        oriented.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
        oriented.extend_from_slice(exif);
        oriented.extend_from_slice(&jpeg[2..]);
        assert_eq!(decode(&oriented).unwrap().dimensions(), (12, 32));
        for bytes in [
            &jpeg[..],
            &oriented[..],
            include_bytes!("test_fixtures/signature-progressive.jpg"),
            include_bytes!("test_fixtures/signature-grayscale.jpg"),
        ] {
            assert!(decode(bytes).is_ok());
            // Guard Cargo's printpdf feature as well as our shared validator.
            assert!(printpdf::image::RawImage::decode_from_bytes(bytes, &mut Vec::new()).is_ok());
        }
    }
    #[test]
    fn signature_content_validation_rejects_damage_and_unsupported_files() {
        let png = ink();
        let jpeg = jpeg();
        for bytes in [
            vec![],
            b"not an image.jpg".to_vec(),
            b"GIF89a\x01\0\x01\0".to_vec(),
            png[..png.len() - 8].to_vec(),
            jpeg[..jpeg.len() - 10].to_vec(),
            vec![0; MAX_BYTES + 1],
        ] {
            assert!(decode(&bytes).is_err());
        }
        let mut damaged = png.clone();
        damaged[45] ^= 0xff;
        assert!(decode(&damaged).is_err());
        let mut damaged = jpeg[..jpeg.len() / 2].to_vec();
        damaged.extend([0xff, 0xd9]);
        assert!(decode(&damaged).is_err());
        let mut oversized = Cursor::new(Vec::new());
        DynamicImage::ImageLuma8(image::GrayImage::new(MAX_DIMENSION + 1, 1))
            .write_to(&mut oversized, ImageFormat::Png)
            .unwrap();
        assert!(decode(&oversized.into_inner()).is_err());
    }
    #[test]
    fn signature_renamed_content_and_failed_replacement_preserve_reference() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("PNG-NAMED.JPG");
        std::fs::write(&path, ink()).unwrap();
        let mut reference = Some("original.png".into());
        select_import(&mut reference, &path).unwrap();
        let previous = reference.clone();
        std::fs::write(&path, b"broken JPEG").unwrap();
        assert!(select_import(&mut reference, &path).is_err());
        assert_eq!(reference, previous);
        assert!(select_import(&mut reference, &dir.path().join("absent.png")).is_err());
        assert_eq!(reference, previous);
    }
    #[test]
    fn signature_missing_unsigned_continuation_keeps_saved_path() {
        let db = database();
        let path = Owner::Employer(1).path(&db).unwrap();
        assert!(checked_path(path.as_deref(), false).is_err());
        assert!(checked_path(path.as_deref(), true).unwrap().is_none());
        assert_eq!(Owner::Employer(1).path(&db).unwrap(), path);
        assert!(checked_path(None, false).unwrap().is_none());
    }
    #[test]
    fn signature_drawn_black_transparent_persistent_owner_specific_and_stale_safe() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = database();
        let bytes = ink();
        let image = decode(&bytes).unwrap().to_rgba8();
        assert!(image.pixels().any(|p| p[3] == 0));
        assert!(image.pixels().any(|p| p[3] == 255));
        assert!(image.pixels().all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0));
        assert!(save_drawn(
            &mut db,
            dir.path(),
            Owner::Employer(1),
            Some("previous.png"),
            &bytes,
            false
        )
        .is_err());
        let employer = save_drawn(
            &mut db,
            dir.path(),
            Owner::Employer(1),
            Some("previous.png"),
            &bytes,
            true,
        )
        .unwrap();
        assert_eq!(std::fs::read(&employer).unwrap(), bytes);
        assert_eq!(
            Owner::Pa(1).path(&db).unwrap().as_deref(),
            Some("pa-old.png")
        );
        assert!(save_drawn(
            &mut db,
            dir.path(),
            Owner::Employer(1),
            Some("previous.png"),
            &bytes,
            true
        )
        .is_err());
        assert_eq!(
            Owner::Employer(1).path(&db).unwrap().as_deref(),
            Some(employer.as_str())
        );
        let pa = save_drawn(
            &mut db,
            dir.path(),
            Owner::Pa(1),
            Some("pa-old.png"),
            &bytes,
            true,
        )
        .unwrap();
        assert_ne!(employer, pa);
        assert_eq!(Owner::Pa(2).path(&db).unwrap(), None);
        assert_eq!(
            checked_path(Some(&pa), false).unwrap(),
            Some(PathBuf::from(&pa))
        );
        let repeated =
            save_drawn(&mut db, dir.path(), Owner::Pa(1), Some(&pa), &bytes, true).unwrap();
        assert_ne!(pa, repeated);
        assert_eq!(Path::new(&repeated).file_name().unwrap(), "PA One (2).png");
        assert_eq!(std::fs::read(pa).unwrap(), bytes);
    }
    #[test]
    fn signature_save_failure_keeps_previous_reference_and_assets() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("unusable");
        std::fs::write(&root, b"file, not a directory").unwrap();
        let mut db = database();
        let bytes = ink();
        assert!(save_drawn(
            &mut db,
            &root,
            Owner::Employer(1),
            Some("previous.png"),
            &bytes,
            true
        )
        .is_err());
        assert_eq!(
            Owner::Employer(1).path(&db).unwrap().as_deref(),
            Some("previous.png")
        );
        assert!(save_drawn(
            &mut db,
            dir.path(),
            Owner::Pa(1),
            Some("pa-old.png"),
            b"bad",
            true
        )
        .is_err());
        assert_eq!(
            Owner::Pa(1).path(&db).unwrap().as_deref(),
            Some("pa-old.png")
        );
    }
    #[test]
    fn signature_drawing_empty_and_wrong_owner_fail_without_changes() {
        assert!(rasterize(&[]).is_err());
        let mut db = database();
        let dir = tempfile::tempdir().unwrap();
        assert!(save_drawn(&mut db, dir.path(), Owner::Employer(99), None, &ink(), true).is_err());
        assert!(!dir.path().join("signatures").exists());
    }
    #[test]
    fn signature_save_respects_cross_instance_dispatch_lock() {
        let (dir, app) = crate::payroll_timesheet_screen::tests::test_application();
        let mut first = crate::payroll_evidence::open(&app).unwrap();
        first
            .execute("INSERT INTO employers(id,name) VALUES(1,'Owner')", [])
            .unwrap();
        let second = crate::payroll_evidence::open(&app).unwrap();
        let lock = crate::timesheet_delivery::production_lock(&second).unwrap();
        assert!(save_drawn(
            &mut first,
            dir.path(),
            Owner::Employer(1),
            None,
            &ink(),
            true
        )
        .is_err());
        assert_eq!(Owner::Employer(1).path(&first).unwrap(), None);
        drop(lock);
        assert!(save_drawn(
            &mut first,
            dir.path(),
            Owner::Employer(1),
            None,
            &ink(),
            true
        )
        .is_ok());
    }
    #[test]
    fn signature_authorisation_uses_database_owner_and_maintenance_is_stale_safe() {
        let (dir, app) = crate::payroll_timesheet_screen::tests::test_application();
        let mut db = crate::payroll_evidence::open(&app).unwrap();
        db.execute(
            "INSERT INTO employers(id,name) VALUES(1,'Actual owner')",
            [],
        )
        .unwrap();
        let mut ui = DrawingUi::default();
        ui.open(Owner::Employer(1), &db).unwrap();
        assert_eq!(ui.drawing.as_ref().unwrap().name, "Employer Actual owner");
        assert!(!ui.drawing.as_ref().unwrap().authorised);
        let path = save_drawn(&mut db, dir.path(), Owner::Employer(1), None, &ink(), true).unwrap();
        assert!(edit_guard(&app, Some(Owner::Employer(1)), None).is_err());
        assert!(edit_guard(&app, Some(Owner::Employer(1)), Some(&path)).is_ok());
        drop(db);
        let reopened = crate::payroll_evidence::open(&app).unwrap();
        assert_eq!(
            Owner::Employer(1).path(&reopened).unwrap().as_deref(),
            Some(path.as_str())
        );
        assert_eq!(read(Path::new(&path)).unwrap(), ink());
    }
    #[test]
    fn signature_embedded_exif_eoi_cannot_hide_primary_truncation() {
        let jpeg = jpeg();
        let mut bytes = vec![0xff, 0xd8, 0xff, 0xe1, 0, 10];
        bytes.extend_from_slice(b"Exif\0\0\xff\xd9");
        bytes.extend_from_slice(&jpeg[2..jpeg.len() - 2]);
        assert!(decode(&bytes).is_err());
    }
    #[test]
    fn signature_renamed_or_replaced_owner_requires_new_authorisation() {
        let mut db = database();
        let dir = tempfile::tempdir().unwrap();
        let mut drawing = DrawingUi::default();
        drawing.open(Owner::Pa(1), &db).unwrap();
        let name = drawing.drawing.as_ref().unwrap().name.clone();
        db.execute(
            "UPDATE personal_assistants SET first_name='Different' WHERE id=1",
            [],
        )
        .unwrap();
        assert!(save_drawn_checked(
            &mut db,
            dir.path(),
            Owner::Pa(1),
            Some("pa-old.png"),
            &ink(),
            true,
            &name
        )
        .is_err());
        assert_eq!(
            Owner::Pa(1).path(&db).unwrap().as_deref(),
            Some("pa-old.png")
        );
        assert!(!dir.path().join("signatures").exists());
    }
    #[test]
    fn signature_filenames_use_full_database_names_and_sequential_numbers() {
        let mut db = database();
        let dir = tempfile::tempdir().unwrap();
        let bytes = ink();
        db.execute(
            "UPDATE employers SET name='Robin Anne O’Connor-Smith' WHERE id=1",
            [],
        )
        .unwrap();
        db.execute("UPDATE personal_assistants SET first_name='Alex Marie',surname='Taylor Jones' WHERE id=1",[]).unwrap();
        let first = save_drawn(
            &mut db,
            dir.path(),
            Owner::Employer(1),
            Some("previous.png"),
            &bytes,
            true,
        )
        .unwrap();
        assert_eq!(
            Path::new(&first).file_name().unwrap(),
            "Robin Anne O’Connor-Smith.png"
        );
        let second = save_drawn(
            &mut db,
            dir.path(),
            Owner::Employer(1),
            Some(&first),
            &bytes,
            true,
        )
        .unwrap();
        let third = save_drawn(
            &mut db,
            dir.path(),
            Owner::Employer(1),
            Some(&second),
            &bytes,
            true,
        )
        .unwrap();
        assert_eq!(
            Path::new(&second).file_name().unwrap(),
            "Robin Anne O’Connor-Smith (2).png"
        );
        assert_eq!(
            Path::new(&third).file_name().unwrap(),
            "Robin Anne O’Connor-Smith (3).png"
        );
        assert_eq!(std::fs::read(&first).unwrap(), bytes);
        let pa = save_drawn(
            &mut db,
            dir.path(),
            Owner::Pa(1),
            Some("pa-old.png"),
            &bytes,
            true,
        )
        .unwrap();
        assert_eq!(
            Path::new(&pa).file_name().unwrap(),
            "Alex Marie Taylor Jones.png"
        );
    }
    #[test]
    fn signature_filenames_fall_back_only_when_names_are_unavailable() {
        let mut db = database();
        let dir = tempfile::tempdir().unwrap();
        db.execute("UPDATE employers SET name='  ' WHERE id=1", [])
            .unwrap();
        db.execute(
            "UPDATE personal_assistants SET first_name='',surname='' WHERE id=1",
            [],
        )
        .unwrap();
        let employer = save_drawn(
            &mut db,
            dir.path(),
            Owner::Employer(1),
            Some("previous.png"),
            &ink(),
            true,
        )
        .unwrap();
        let pa = save_drawn(
            &mut db,
            dir.path(),
            Owner::Pa(1),
            Some("pa-old.png"),
            &ink(),
            true,
        )
        .unwrap();
        assert_eq!(Path::new(&employer).file_name().unwrap(), "Employer 1.png");
        assert_eq!(Path::new(&pa).file_name().unwrap(), "PA 1.png");
    }
    #[test]
    fn signature_same_name_owners_keep_separate_assets_and_references() {
        let mut db = database();
        let dir = tempfile::tempdir().unwrap();
        db.execute("UPDATE employers SET name='Alex Taylor' WHERE id=1", [])
            .unwrap();
        db.execute(
            "UPDATE personal_assistants SET first_name='Alex',surname='Taylor'",
            [],
        )
        .unwrap();
        let first = save_drawn(
            &mut db,
            dir.path(),
            Owner::Employer(1),
            Some("previous.png"),
            &ink(),
            true,
        )
        .unwrap();
        let other_ink = rasterize(&[vec![egui::pos2(0.2, 0.2), egui::pos2(0.3, 0.8)]]).unwrap();
        let second = save_drawn(
            &mut db,
            dir.path(),
            Owner::Pa(1),
            Some("pa-old.png"),
            &other_ink,
            true,
        )
        .unwrap();
        let third = save_drawn(&mut db, dir.path(), Owner::Pa(2), None, &ink(), true).unwrap();
        for (path, name) in [
            (&first, "Alex Taylor.png"),
            (&second, "Alex Taylor (2).png"),
            (&third, "Alex Taylor (3).png"),
        ] {
            assert_eq!(Path::new(path).file_name().unwrap(), name);
        }
        assert_eq!(
            Owner::Employer(1).path(&db).unwrap().as_deref(),
            Some(first.as_str())
        );
        assert_eq!(
            Owner::Pa(1).path(&db).unwrap().as_deref(),
            Some(second.as_str())
        );
        assert_eq!(std::fs::read(&first).unwrap(), ink());
        assert_eq!(std::fs::read(&second).unwrap(), other_ink);
        assert!(save_drawn(
            &mut db,
            dir.path(),
            Owner::Pa(1),
            Some(&first),
            &ink(),
            true
        )
        .is_err());
    }
    #[test]
    fn signature_filename_collisions_are_unicode_caseless_and_include_nonfiles() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("JOSÉ.PNG"), b"retain me").unwrap();
        std::fs::create_dir(dir.path().join("José (2).png")).unwrap();
        assert_eq!(
            next_filename(dir.path(), "Jose\u{301}")
                .unwrap()
                .file_name()
                .unwrap(),
            "Jose\u{301} (3).png"
        );
        assert_eq!(
            std::fs::read(dir.path().join("JOSÉ.PNG")).unwrap(),
            b"retain me"
        );
        assert_eq!(collision_key("Straße.png"), collision_key("STRASSE.PNG"));
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("missing", dir.path().join("JOSE\u{301} (3).PNG")).unwrap();
            assert_eq!(
                next_filename(dir.path(), "José")
                    .unwrap()
                    .file_name()
                    .unwrap(),
                "José (4).png"
            );
        }
    }
    #[test]
    fn signature_filename_sanitising_preserves_unicode_and_legitimate_punctuation() {
        assert_eq!(
            filename_stem("  Zoë 李 O'Connor-Smith  "),
            "Zoë 李 O'Connor-Smith"
        );
        assert_eq!(
            filename_stem("Alex / Taylor: Jr?*<>\"|\\\n. "),
            "Alex - Taylor- Jr--------"
        );
        for name in [
            "CON", "NUL.txt", "COM1", "LPT9", "COM¹", "CON .txt", "CONIN$", "CONOUT$",
        ] {
            assert!(filename_stem(name).starts_with('_'));
        }
        assert_eq!(filename_stem("CONstance"), "CONstance");
        assert_eq!(filename_stem(".."), "_");
        let long = "李".repeat(100);
        let short = filename_stem(&long);
        assert!(short.len() <= 180);
        assert!(long.starts_with(&short));
    }
    #[test]
    fn signature_legacy_hash_and_external_references_are_never_renamed() {
        let mut db = database();
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join(
            "employer-1-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef.png",
        );
        let external = dir.path().join("external-signature.png");
        std::fs::write(&legacy, ink()).unwrap();
        std::fs::write(&external, ink()).unwrap();
        db.execute(
            "UPDATE employers SET employer_signature=?1 WHERE id=1",
            [legacy.to_str().unwrap()],
        )
        .unwrap();
        db.execute(
            "UPDATE personal_assistants SET signature=?1 WHERE id=1",
            [external.to_str().unwrap()],
        )
        .unwrap();
        assert!(checked_path(legacy.to_str(), false).unwrap().is_some());
        let saved = save_drawn(
            &mut db,
            dir.path(),
            Owner::Employer(1),
            legacy.to_str(),
            &ink(),
            true,
        )
        .unwrap();
        assert_eq!(Path::new(&saved).file_name().unwrap(), "Employer.png");
        assert_eq!(read(&legacy).unwrap(), ink());
        assert_eq!(read(&external).unwrap(), ink());
        assert_eq!(
            Owner::Pa(1).path(&db).unwrap().as_deref(),
            external.to_str()
        );
    }
}
