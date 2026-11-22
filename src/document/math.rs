mod symbols;
mod unicode;

pub(crate) use symbols::{
    decode_superscript_label, format_html_subscript, format_html_superscript,
};

use crate::{Error, Result};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use image::{ImageFormat, RgbaImage, imageops::FilterType};
use ratex_layout::{LayoutOptions as RatexLayoutOptions, layout, to_display_list, validate_layout};
use ratex_parser::parse;
use ratex_render::{RenderOptions, render_to_png};
use ratex_types::{Color, MathStyle};
use std::io::Cursor;

pub const MATH_IMAGE_SCHEME: &str = "ers-math:";
const MAX_LATEX_BYTES: usize = 16 * 1024;
// #NOTE: KaTeX cap-height is ~0.683em while monospace cap-height is ~0.50 of cell height, so 0.73x cell height matches body text.
const MATH_EM_TO_CELL_HEIGHT: f32 = 0.73;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MathImageRequest {
    pub latex: String,
    pub dark_theme: bool,
    pub columns: u16,
    pub rows: u16,
    pub cell_width_px: u16,
    pub cell_height_px: u16,
}

impl MathImageRequest {
    pub fn with_default_geometry(latex: impl Into<String>, dark_theme: bool) -> Self {
        Self {
            latex: latex.into(),
            dark_theme,
            columns: 80,
            rows: 3,
            cell_width_px: 10,
            cell_height_px: 20,
        }
    }
}

pub fn render_inline_unicode(latex: &str) -> String {
    unicode::render_inline_unicode(latex)
}

pub fn render_display_unicode(latex: &str, max_width: usize) -> Vec<String> {
    unicode::render_display_unicode(latex, max_width)
}

pub fn is_math_image_path(path: &str) -> bool {
    path.starts_with(MATH_IMAGE_SCHEME)
}

pub fn encode_math_image_path(request: &MathImageRequest) -> String {
    let mode = if request.dark_theme { "dark" } else { "light" };
    let encoded = URL_SAFE_NO_PAD.encode(request.latex.as_bytes());
    format!(
        "{MATH_IMAGE_SCHEME}{mode}:{}x{}@{}x{}:{encoded}",
        request.columns, request.rows, request.cell_width_px, request.cell_height_px
    )
}

pub fn decode_math_image_path(path: &str) -> Option<MathImageRequest> {
    let rest = path.strip_prefix(MATH_IMAGE_SCHEME)?;
    let (dark_theme, rest) = if let Some(rest) = rest.strip_prefix("dark:") {
        (true, rest)
    } else if let Some(rest) = rest.strip_prefix("light:") {
        (false, rest)
    } else {
        (true, rest)
    };
    let (columns, rows, cell_width_px, cell_height_px, encoded) =
        if let Some((geometry, encoded)) = rest.split_once(':') {
            let (cells, pixels) = geometry.split_once('@')?;
            let (cols, rows) = cells.split_once('x')?;
            let (cw, ch) = pixels.split_once('x')?;
            (
                cols.parse().ok()?,
                rows.parse().ok()?,
                cw.parse().ok()?,
                ch.parse().ok()?,
                encoded,
            )
        } else {
            (80, 3, 10, 20, rest)
        };
    let bytes = URL_SAFE_NO_PAD.decode(encoded).ok()?;
    let latex = String::from_utf8(bytes).ok()?;
    Some(MathImageRequest {
        latex,
        dark_theme,
        columns,
        rows,
        cell_width_px,
        cell_height_px,
    })
}

// #NOTE: Standalone <span class="katex"> equations inside a prose-free block often omit katex-display; promote non-trivial equations while keeping bare symbols inline.
pub fn is_standalone_display_candidate(latex: &str) -> bool {
    let trimmed = latex.trim();
    if trimmed.len() >= 20 {
        return true;
    }
    trimmed.contains('=')
        || trimmed.contains(r"\frac")
        || trimmed.contains(r"\sum")
        || trimmed.contains(r"\int")
        || trimmed.contains(r"\prod")
        || trimmed.contains(r"\sqrt")
        || trimmed.contains(r"\begin{")
        || trimmed.contains(r"\qquad")
}

pub fn render_display_png(latex: &str, dark_theme: bool) -> Result<Vec<u8>> {
    render_display_png_for_request(&MathImageRequest::with_default_geometry(latex, dark_theme))
}

pub fn render_display_png_for_request(request: &MathImageRequest) -> Result<Vec<u8>> {
    let trimmed = request.latex.trim();
    if trimmed.is_empty() {
        return Err(Error::Compatibility("The math expression is empty.".into()));
    }
    if trimmed.len() > MAX_LATEX_BYTES {
        return Err(Error::Compatibility(
            "The math expression exceeds the size limit.".into(),
        ));
    }
    let ast = parse(trimmed)
        .map_err(|error| Error::Compatibility(format!("Cannot parse LaTeX math: {error}")))?;
    let foreground = if request.dark_theme {
        Color::new(0.90, 0.90, 0.90, 1.0)
    } else {
        Color::new(0.11, 0.11, 0.11, 1.0)
    };
    let options = RatexLayoutOptions::default()
        .with_style(MathStyle::Display)
        .with_color(foreground);
    let layout_box = layout(&ast, &options);
    validate_layout(&layout_box)
        .map_err(|error| Error::Compatibility(format!("Math layout exceeded limits: {error}")))?;
    let display_list = to_display_list(&layout_box);

    let cell_w = u32::from(request.cell_width_px).max(1);
    let cell_h = u32::from(request.cell_height_px).max(1);
    let max_cols = u32::from(request.columns).clamp(1, 296);
    let max_rows = u32::from(request.rows).clamp(1, 296);
    let max_w = (max_cols * cell_w).min(2048);
    let max_h = (max_rows * cell_h).min(2048);

    let base_em = (cell_h as f32 * MATH_EM_TO_CELL_HEIGHT).max(8.0);
    let pad = (base_em * 0.12).clamp(2.0, 10.0);
    let avail_w = (max_w as f32 - 2.0 * pad).max(1.0);
    let avail_h = (max_h as f32 - 2.0 * pad).max(1.0);
    let expr_w = (display_list.width.max(0.1) as f32) * base_em;
    let expr_h = ((display_list.height + display_list.depth).max(0.1) as f32) * base_em;
    let scale = (avail_w / expr_w).min(avail_h / expr_h).min(1.0);
    let em_px = (base_em * scale).max(6.0);

    let render_options = RenderOptions {
        font_size: em_px,
        padding: pad,
        background_color: Color::new(0.0, 0.0, 0.0, 0.0),
        font_dir: String::new(),
        device_pixel_ratio: 1.0,
    };
    let raw_png = render_to_png(&display_list, &render_options)
        .map_err(|error| Error::Compatibility(format!("Cannot rasterize LaTeX math: {error}")))?;
    align_math_png(&raw_png, cell_w, cell_h, max_cols, max_rows)
}

fn align_math_png(
    raw_png: &[u8],
    cell_w: u32,
    cell_h: u32,
    max_cols: u32,
    max_rows: u32,
) -> Result<Vec<u8>> {
    let mut rendered = image::load_from_memory_with_format(raw_png, ImageFormat::Png)
        .map_err(|error| Error::Compatibility(format!("Cannot decode math PNG: {error}")))?;
    let max_w = (max_cols * cell_w).min(2048);
    let max_h = (max_rows * cell_h).min(2048);
    if rendered.width() > max_w || rendered.height() > max_h {
        rendered = rendered.resize(max_w, max_h, FilterType::Triangle);
    }
    let rgba = rendered.to_rgba8();
    let cols = rgba.width().div_ceil(cell_w).clamp(1, max_cols);
    let canvas_w = (cols * cell_w).min(max_w).max(rgba.width());
    let canvas_h = max_h.max(rgba.height());
    let mut canvas = RgbaImage::new(canvas_w, canvas_h);
    let offset_x = (canvas_w - rgba.width()) / 2;
    let offset_y = (canvas_h - rgba.height()) / 2;
    image::imageops::overlay(&mut canvas, &rgba, i64::from(offset_x), i64::from(offset_y));
    let mut out = Cursor::new(Vec::new());
    canvas
        .write_to(&mut out, ImageFormat::Png)
        .map_err(|error| {
            Error::Compatibility(format!("Cannot encode aligned math PNG: {error}"))
        })?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_inline_unicode_for_real_equations() {
        assert_eq!(render_inline_unicode(r"\lambda"), "λ");
        assert_eq!(render_inline_unicode(r"\beta (> 0)"), "β(> 0)");
        let lom = render_inline_unicode(
            r"\,\dot{S}(L, C) = a[R(L, C)]^{\lambda} S^{1-\beta} \qquad \text{(Software L.O.M)}",
        );
        assert!(lom.contains("S\u{0307}(L, C) = a[R(L, C)]"), "{lom}");
        assert!(lom.contains("S¹⁻ᵝ"), "{lom}");
        assert!(lom.contains("(Software L.O.M)"), "{lom}");

        let cog = render_inline_unicode(r"r_{\mathrm{cog}}");
        assert!(cog.contains("r") && cog.contains("cog"), "{cog}");
    }

    #[test]
    fn renders_display_2d_fraction_and_stays_within_width() {
        let lines = render_display_unicode(r"\frac{-b \pm \sqrt{b^2 - 4ac}}{2a}", 40);
        assert!(lines.len() >= 3, "{lines:?}");
        assert!(lines.iter().any(|line| line.contains('─')), "{lines:?}");
        assert!(
            lines
                .iter()
                .all(|line| crate::document::text::cell_width(line) <= 40),
            "{lines:?}"
        );

        let narrow = render_display_unicode(r"\frac{-b \pm \sqrt{b^2 - 4ac}}{2a}", 12);
        assert!(
            narrow
                .iter()
                .all(|line| crate::document::text::cell_width(line) <= 12),
            "{narrow:?}"
        );
    }

    #[test]
    fn rasterizes_display_math_to_png_and_roundtrips_path() {
        let tex = r"\text{Doubling Time}(2S, C) = \text{Doubling Time}(S, C) \times 2^{p(\frac{1}{r_{\mathrm{cog}}}-1)}";
        let png = render_display_png(tex, true).unwrap();
        assert!(png.starts_with(b"\x89PNG"));

        let req = MathImageRequest {
            latex: tex.into(),
            dark_theme: false,
            columns: 72,
            rows: 3,
            cell_width_px: 16,
            cell_height_px: 32,
        };
        let path = encode_math_image_path(&req);
        assert!(is_math_image_path(&path));
        let decoded = decode_math_image_path(&path).unwrap();
        assert_eq!(decoded, req);
    }
}
