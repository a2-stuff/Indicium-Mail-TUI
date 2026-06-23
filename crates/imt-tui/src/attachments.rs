//! Attachment classification, text extraction (PDF / DOCX / text), and image
//! rendering to terminal half-block cells (works over SSH in any truecolor
//! terminal - no graphics protocol required).

use std::path::Path;

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

/// Broad category of an attachment, used to choose how to display it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentKind {
    Text,
    Image,
    Pdf,
    Docx,
    Other,
}

/// Classify an attachment by MIME type and filename extension.
pub fn classify(mime: &str, filename: &str) -> AttachmentKind {
    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let m = mime.to_ascii_lowercase();

    if m.starts_with("image/") || matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp") {
        return AttachmentKind::Image;
    }
    if m == "application/pdf" || ext == "pdf" {
        return AttachmentKind::Pdf;
    }
    if ext == "docx" || m.contains("wordprocessingml") {
        return AttachmentKind::Docx;
    }
    if m.starts_with("text/") || crate::app::is_viewable_by_name(filename) {
        return AttachmentKind::Text;
    }
    AttachmentKind::Other
}

/// True if this attachment can be viewed inline (image, pdf, docx, or text).
pub fn is_viewable(mime: &str, filename: &str) -> bool {
    !matches!(classify(mime, filename), AttachmentKind::Other)
}

/// Extract readable text from a text/PDF/DOCX attachment.
pub fn extract_text(path: &Path, kind: AttachmentKind) -> Result<String, String> {
    match kind {
        AttachmentKind::Pdf => extract_pdf(path),
        AttachmentKind::Docx => extract_docx(path),
        AttachmentKind::Text => std::fs::read(path)
            .map_err(|e| e.to_string())
            .map(|b| String::from_utf8_lossy(&b).into_owned()),
        _ => Err("attachment is not text".into()),
    }
}

fn extract_pdf(path: &Path) -> Result<String, String> {
    match pdf_extract::extract_text(path) {
        Ok(t) => {
            let t = t.trim().to_string();
            if t.is_empty() {
                Ok("[No extractable text - this PDF may be scanned / image-only.]".to_string())
            } else {
                Ok(t)
            }
        }
        Err(e) => Err(format!("PDF extract failed: {e}")),
    }
}

fn extract_docx(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("not a valid .docx: {e}"))?;
    let mut xml = String::new();
    {
        let mut doc = zip
            .by_name("word/document.xml")
            .map_err(|_| "word/document.xml not found in .docx".to_string())?;
        doc.read_to_string(&mut xml).map_err(|e| e.to_string())?;
    }
    Ok(docx_xml_to_text(&xml))
}

/// Convert WordprocessingML body XML into plain text: paragraph and break tags
/// become newlines, tabs become tabs, all other tags are stripped.
fn docx_xml_to_text(xml: &str) -> String {
    let pre = xml
        .replace("</w:p>", "\n")
        .replace("<w:br/>", "\n")
        .replace("<w:br />", "\n")
        .replace("<w:tab/>", "\t")
        .replace("<w:tab />", "\t");

    let mut out = String::with_capacity(pre.len());
    let mut in_tag = false;
    for ch in pre.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'");
    let joined = out
        .lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n");
    let trimmed = joined.trim().to_string();
    if trimmed.is_empty() {
        "[No extractable text in document]".to_string()
    } else {
        trimmed
    }
}

/// Decode an image file into memory.
pub fn load_image(path: &Path) -> Result<image::DynamicImage, String> {
    image::open(path).map_err(|e| format!("could not decode image: {e}"))
}

/// Render an image to half-block (`▀`) lines fitting within `cols` x `rows`
/// terminal cells. Each cell stacks two pixels: top pixel as foreground, bottom
/// as background. Aspect ratio is preserved (one cell ≈ 1px wide x 2px tall).
pub fn image_to_lines(img: &image::DynamicImage, cols: u16, rows: u16) -> Vec<Line<'static>> {
    use image::GenericImageView;

    if cols == 0 || rows == 0 {
        return Vec::new();
    }
    let (iw, ih) = img.dimensions();
    if iw == 0 || ih == 0 {
        return Vec::new();
    }

    let target_w = cols as f64;
    let target_h = (rows as f64) * 2.0; // two pixels per cell vertically
    let scale = (target_w / iw as f64).min(target_h / ih as f64);
    let new_w = ((iw as f64) * scale).round().max(1.0) as u32;
    let mut new_h = ((ih as f64) * scale).round().max(2.0) as u32;
    if new_h % 2 == 1 {
        new_h += 1; // even height so every cell has a top+bottom pixel
    }

    let resized = img
        .resize_exact(new_w, new_h, image::imageops::FilterType::Triangle)
        .to_rgba8();
    let w = resized.width();
    let h = resized.height();

    let mut lines = Vec::with_capacity((h / 2) as usize);
    let mut y = 0;
    while y < h {
        let mut spans: Vec<Span<'static>> = Vec::with_capacity(w as usize);
        for x in 0..w {
            let top = resized.get_pixel(x, y).0;
            let bottom = if y + 1 < h {
                resized.get_pixel(x, y + 1).0
            } else {
                top
            };
            let fg = Color::Rgb(top[0], top[1], top[2]);
            let bg = Color::Rgb(bottom[0], bottom[1], bottom[2]);
            spans.push(Span::styled("\u{2580}", Style::default().fg(fg).bg(bg)));
        }
        lines.push(Line::from(spans));
        y += 2;
    }
    lines
}
