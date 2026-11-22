use ers::book::BookLocation;
use ers::document::layout::{LayoutOptions, layout_document};
use ers::document::math::{
    MathImageRequest, decode_math_image_path, encode_math_image_path, render_display_png,
    render_display_png_for_request, render_display_unicode, render_inline_unicode,
};
use ers::document::text::{ParagraphRole, cell_width, parse_document};
use ers::images::{ImageJob, ImageTarget, ImageWorker, ImageWorkerOptions};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[test]
fn deduplicates_katex_inline_and_promotes_standalone_equations() {
    let html = r#"
    <html><body>
      <p>The model simulates AI software.</p>
      <div data-component="Markdown-p">
        <span class="katex">
          <span class="katex-mathml"><math><semantics><mrow>Ṡ(L,C)=a[R(L,C)]λS1-β</mrow><annotation encoding="application/x-tex">\,\dot{S}(L, C) = a[R(L, C)]^{\lambda} S^{1-\beta} \qquad \text{(Software L.O.M)}</annotation></semantics></math></span>
          <span class="katex-html" aria-hidden="true"><span>Ṡ(L,C)=a[R(L,C)]λS1-β</span></span>
        </span>
        <span class="katex">
          <span class="katex-mathml"><math><semantics><mrow>R(L,C)=(bL)α(cC)1-α</mrow><annotation encoding="application/x-tex">R(L, C) = (bL)^{\alpha}(cC)^{1-\alpha}</annotation></semantics></math></span>
          <span class="katex-html" aria-hidden="true"><span>R(L,C)=(bL)α(cC)1-α</span></span>
        </span>
      </div>
      <ul>
        <li><span class="katex"><span class="katex-mathml"><math><semantics><mrow>S</mrow><annotation encoding="application/x-tex">S</annotation></semantics></math></span><span class="katex-html" aria-hidden="true"><span>S</span></span></span> is the level of AI software</li>
        <li><span class="katex"><span class="katex-mathml"><math><semantics><mrow>λ</mrow><annotation encoding="application/x-tex">\lambda</annotation></semantics></math></span><span class="katex-html" aria-hidden="true"><span>λ</span></span></span> captures stepping on toes</li>
      </ul>
    </body></html>
    "#;

    let parsed = parse_document("_", html).unwrap();
    let math_paragraphs: Vec<_> = parsed
        .paragraphs
        .iter()
        .filter(|paragraph| paragraph.role == ParagraphRole::Math)
        .collect();
    assert_eq!(math_paragraphs.len(), 2, "{:#?}", parsed.paragraphs);
    assert!(
        math_paragraphs[0]
            .math_latex
            .as_deref()
            .unwrap()
            .contains(r"\dot{S}")
    );
    assert!(
        math_paragraphs[1]
            .math_latex
            .as_deref()
            .unwrap()
            .contains(r"(bL)^{\alpha}")
    );

    let full_text = parsed.source.text.as_str();
    assert!(!full_text.contains("SSS"), "{full_text}");
    assert!(!full_text.contains(r"\lambda"), "{full_text}");
    assert!(
        full_text.contains("S is the level of AI software"),
        "{full_text}"
    );
    assert!(
        full_text.contains("λ captures stepping on toes"),
        "{full_text}"
    );

    let layout = layout_document(&parsed, LayoutOptions::reader(72, true));
    assert_eq!(layout.math_slots.len(), 2);
    assert!(layout.math_slots.iter().all(|slot| slot.rows >= 3));
    assert!(layout.text_lines.iter().all(|line| cell_width(line) <= 72));
}

#[test]
fn handles_explicit_display_math_and_remaps_slots_after_images() {
    let html = r#"
    <html><body>
      <img src="diagram.png" />
      <p>Before inline <math><semantics><annotation encoding="application/x-tex">e^{i\pi} + 1 = 0</annotation></semantics></math> after.</p>
      <span class="katex-display">
        <span class="katex">
          <span class="katex-mathml"><math><semantics><annotation encoding="application/x-tex">\frac{1}{r_{\mathrm{cog}}} - 1</annotation></semantics></math></span>
          <span class="katex-html" aria-hidden="true"><span>dup</span></span>
        </span>
      </span>
    </body></html>
    "#;

    let parsed = parse_document("_", html).unwrap();
    assert!(
        parsed
            .source
            .text
            .contains("Before inline eⁱ^π + 1 = 0 after.")
            || parsed
                .source
                .text
                .contains("Before inline e^(iπ) + 1 = 0 after.")
            || parsed.source.text.contains("Before inline e")
    );
    assert!(!parsed.source.text.contains("dup"));

    let compact = layout_document(&parsed, LayoutOptions::reader(48, false));
    let expanded = layout_document(&parsed, LayoutOptions::reader(48, true));
    assert_eq!(compact.math_slots.len(), 1);
    assert_eq!(expanded.math_slots.len(), 1);
    // Image slot expansion shifts the math slot downward by image_rows - 1.
    let delta = expanded.text_lines.image_layout().height() - 1;
    assert_eq!(
        expanded.math_slots[0].row,
        compact.math_slots[0].row + delta
    );
    assert!(
        expanded
            .text_lines
            .iter()
            .all(|line| cell_width(line) <= 48)
    );
}

#[test]
fn image_worker_renders_display_math_png_in_process() {
    let tex = r"\sum_{n=1}^{\infty} \frac{1}{n^2} = \frac{\pi^2}{6}";
    let request = MathImageRequest {
        latex: tex.into(),
        dark_theme: true,
        columns: 72,
        rows: 3,
        cell_width_px: 16,
        cell_height_px: 32,
    };
    let path = encode_math_image_path(&request);
    let decoded_req = decode_math_image_path(&path).unwrap();
    assert_eq!(decoded_req.latex, tex);
    assert!(decoded_req.dark_theme);

    let direct_png = render_display_png(tex, true).unwrap();
    assert!(direct_png.starts_with(b"\x89PNG"));

    let worker = ImageWorker::start(ImageWorkerOptions::new(PathBuf::from("/nonexistent")));
    let _gen = worker.request(
        BookLocation::Local(PathBuf::from("/nonexistent.epub")),
        ImageJob {
            request_id: 1,
            target: ImageTarget {
                content_id: "_".into(),
                path,
                row_offset: 0,
            },
        },
    );

    let deadline = Instant::now() + Duration::from_secs(10);
    let result = loop {
        if let Some(res) = worker.take_result() {
            break res;
        }
        assert!(Instant::now() < deadline, "Timed out waiting for math PNG");
        std::thread::sleep(Duration::from_millis(10));
    };
    let payload = result.result.unwrap();
    assert!(payload.bytes.starts_with(b"\x89PNG"));
    let decoded = payload.decoded.as_ref().unwrap();
    assert!(decoded.pixels.width() > 0 && decoded.pixels.height() > 0);

    // Also verify 2D Unicode fallback for the same equation.
    let lines = render_display_unicode(tex, 60);
    assert!(lines.len() >= 3, "{lines:?}");
    assert!(lines.iter().any(|line| line.contains('∑')), "{lines:?}");
    assert!(lines.iter().any(|line| line.contains('─')), "{lines:?}");
    assert_eq!(render_inline_unicode(r"\alpha + \beta"), "α + β");
}

#[test]
fn short_display_equations_do_not_upscale_past_terminal_font_size() {
    let short_req = MathImageRequest {
        latex: r"L = dS".into(),
        dark_theme: true,
        columns: 72,
        rows: 3,
        cell_width_px: 16,
        cell_height_px: 32,
    };
    let png = render_display_png_for_request(&short_req).unwrap();
    let img = image::load_from_memory(&png).unwrap().to_rgba8();
    // Exact cell alignment: width is a small multiple of cell_width_px (not full 72 cols), height is rows * cell_height_px.
    assert_eq!(img.width() % 16, 0);
    assert_eq!(img.height(), 3 * 32);
    assert!(
        img.width() < 20 * 16,
        "short equation should not stretch to pane width: {}px",
        img.width()
    );
    // Ink bounding box height should match ~1 terminal cell height, not stretch to 3 rows.
    let (mut min_y, mut max_y) = (u32::MAX, 0u32);
    for (x, y, px) in img.enumerate_pixels() {
        let _ = x;
        if px.0[3] > 20 {
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
    }
    let ink_height = max_y.saturating_sub(min_y) + 1;
    assert!(
        ink_height <= 32,
        "single-line equation ink height ({ink_height}px) should not exceed 1 cell height (32px)"
    );
}

#[test]
fn sup_with_nested_sr_only_heading_stays_inline_without_splitting() {
    let html = r##"<ul><li><div>We use <span class="katex"><span class="katex-mathml"><math><semantics><annotation encoding="application/x-tex">\lambda = 0.6</annotation></semantics></math></span><span class="katex-html" aria-hidden="true"><span>dup</span></span></span>.<sup><a href="#fn-23">23</a><h2 class="sr-only"></h2></sup></div></li></ul>"##;
    let parsed = parse_document("_", html).unwrap();
    let text = parsed.source.text.as_str();
    assert!(text.contains("We use λ = 0.6.²³"), "{text}");
    assert!(!text.contains("^{"), "{text}");
    assert!(!text.contains("\n}"), "{text}");
}

#[test]
fn arxiv_sup_and_ltx_notes_render_unicode_superscripts_without_duplication() {
    let html = r##"<h1>Scaling Discovery
Jongho Park<sup class="ltx_sup"><span>b</span></sup><span class="ltx_note ltx_role_footnote"><sup class="ltx_note_mark">1</sup><span class="ltx_note_outer"><span class="ltx_note_content"><sup class="ltx_note_mark">1</sup><span class="ltx_tag ltx_tag_note">1</span> This work was done during an internship.</span></span></span> Vasilis Kontonis<sup class="ltx_sup"><span>m</span></sup></h1>
<span class="ltx_note ltx_role_footnotetext"><sup class="ltx_note_mark">0</sup><span class="ltx_note_outer"><span class="ltx_note_content"><sup class="ltx_note_mark">0</sup><span class="ltx_note_type">footnotetext: </span>Emails: a@b.edu</span></span></span>"##;
    let parsed = parse_document("arxiv.html", html).unwrap();
    let text = parsed.source.text.as_str();
    assert!(
        text.contains("Jongho Parkᵇ¹ [This work was done during an internship.]"),
        "{text}"
    );
    assert!(text.contains("Vasilis Kontonisᵐ"), "{text}");
    assert!(text.contains("Emails: a@b.edu"), "{text}");
    assert!(!text.contains("^{"), "raw ^{{}} leaked: {text}");
    assert!(
        !text.contains("footnotetext:"),
        "ltx_note_type leaked: {text}"
    );
    assert!(!text.contains("¹¹"), "duplicate note mark leaked: {text}");
}
