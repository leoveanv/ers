use std::collections::BTreeMap;

use ers::document::layout::{FontStyle, LayoutOptions, SourceCell, StyleSpan, layout_document};
use ers::document::links::{DocumentAnchor, TextLink};
use ers::document::text::parse_document;
use serde::Deserialize;

#[derive(Deserialize)]
struct LayoutFixture {
    name: String,
    html: String,
    width: usize,
    starting_row: usize,
    text_lines: Vec<String>,
    source_cells: Vec<SourceCell>,
    formatting: Vec<StyleSpan>,
    image_maps: BTreeMap<usize, String>,
    links: Vec<TextLink>,
    anchors: Vec<DocumentAnchor>,
}

#[test]
fn table_before_heading_wraps_as_body_text() {
    let document = parse_document(
        "outline.html",
        "<h3>Divine Names</h3><table><tr><td>Chs. 1–3:</td> \
         <td>how the divine names do and do not know God</td></tr></table>\n\
         <h3 id='next'>Mystical Theology</h3>",
    )
    .unwrap();
    let layout = layout_document(&document, LayoutOptions::reader(24, false));
    let lines = layout
        .text_lines
        .iter()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    assert_eq!(
        lines,
        [
            "Divine Names",
            "Chs. 1–3: how the divine",
            "names do and do not know",
            "God",
            "Mystical Theology",
            "***",
        ]
    );
    let heading = document
        .semantics
        .anchors
        .iter()
        .find(|anchor| anchor.fragment == "next")
        .unwrap();
    assert_eq!(
        layout.row_for_offset(heading.offset),
        layout
            .text_lines
            .iter()
            .position(|line| line.trim() == "Mystical Theology")
            .unwrap()
    );
}

#[test]
fn void_image_does_not_absorb_following_paragraph() {
    let document = parse_document(
        "article.html",
        "<figure><picture><img src='chart.png' alt='chart'></picture></figure>\
         <p>Extrapolating both trends into the next decade raises the prospect of a dramatically larger global effective compute stock.</p>",
    )
    .unwrap();
    for inline_images in [false, true] {
        let layout = layout_document(&document, LayoutOptions::reader(32, inline_images));
        let image_row = *layout.image_maps.keys().next().unwrap();
        assert_eq!(layout.text_lines[image_row].trim(), "[IMAGE]");
        assert!(
            layout
                .text_lines
                .iter()
                .all(|line| ers::document::text::cell_width(line) <= 32),
            "every laid-out line must fit within the reading width"
        );
        let text_rows = layout
            .text_lines
            .iter()
            .skip(image_row + layout.text_lines.image_layout().height())
            .map(|line| line.trim())
            .filter(|line| !line.is_empty() && *line != "***")
            .collect::<Vec<_>>();
        assert_eq!(
            text_rows.join(" "),
            "Extrapolating both trends into the next decade raises the prospect of a dramatically larger global effective compute stock."
        );
        assert!(layout.source_cells.iter().any(|cell| cell.row
            >= image_row + layout.text_lines.image_layout().height()
            && cell.text == "E"));
    }
}

#[test]
fn long_heading_wraps_within_reading_width() {
    let document = parse_document(
        "article.html",
        "<h4 id='section-3b2'><span class='section-number'>3.B.2</span>          On domains where the ‘true’ quality of a solution is hard for the agent to determine          due to prohibitively long feedback loops or requirements that the solution generalize,          models appear to have worse judgment than human experts</h4>         <p>Body paragraph.</p>",
    )
    .unwrap();
    let width = 48;
    let layout = layout_document(&document, LayoutOptions::reader(width, false));
    assert!(
        layout
            .text_lines
            .iter()
            .all(|line| ers::document::text::cell_width(line) <= width),
        "every laid-out line must fit within the reading width"
    );
    let body_row = layout
        .text_lines
        .iter()
        .position(|line| line.trim() == "Body paragraph.")
        .unwrap();
    let heading_rows = (0..body_row)
        .filter(|row| !layout.text_lines[*row].trim().is_empty())
        .collect::<Vec<_>>();
    assert!(
        heading_rows.len() > 1,
        "long heading should wrap across multiple rows"
    );
    let reconstructed = heading_rows
        .iter()
        .map(|row| layout.text_lines[*row].trim())
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(
        reconstructed,
        "3.B.2 On domains where the ‘true’ quality of a solution is hard for the agent to determine due to prohibitively long feedback loops or requirements that the solution generalize, models appear to have worse judgment than human experts"
    );
    for row in &heading_rows {
        assert!(
            layout
                .formatting
                .iter()
                .any(|span| span.row == *row && span.style == FontStyle::Bold),
            "every wrapped heading row must remain bold"
        );
        assert!(
            !layout.cells_on_row(*row).is_empty(),
            "every wrapped heading row must map back to source cells"
        );
    }
    let anchor = document
        .semantics
        .anchors
        .iter()
        .find(|anchor| anchor.fragment == "section-3b2")
        .unwrap();
    assert_eq!(layout.row_for_offset(anchor.offset), heading_rows[0]);
}

#[test]
fn all_paragraph_roles_fit_within_reading_width() {
    let html = "<figcaption>Unseparated caption before heading</figcaption>                <h2 id='h'>Long heading with 中文 Café 👩‍🔬 and many words that must wrap cleanly</h2>                <section>Trailing section text</section>                <blockquote>Quoted text that wraps across several lines with 中文 characters.</blockquote>                <ul><li>repeat repeat repeat repeat</li></ul>                <pre>const VALUE: &str = 'a_very_long_unbroken_identifier_for_preformatted_wrapping';</pre>                <img src='figure.png' alt='fig'>                <p>Normal paragraph with supercalifragilisticexpialidocious_unbroken_token.</p>";
    let document = parse_document("article.html", html).unwrap();
    for width in [4, 8, 16, 32, 80] {
        let layout = layout_document(&document, LayoutOptions::reader(width, false));
        for (row, line) in layout.text_lines.iter().enumerate() {
            let measured = ers::document::text::cell_width(line);
            assert!(
                measured <= width,
                "width={width} row={row} exceeded width ({measured} > {width}): {line:?}"
            );
        }
    }
    let layout = layout_document(&document, LayoutOptions::reader(24, false));
    let caption_row = layout
        .text_lines
        .iter()
        .position(|line| line.trim().starts_with("Unseparated caption"))
        .unwrap();
    let heading = document
        .semantics
        .anchors
        .iter()
        .find(|anchor| anchor.fragment == "h")
        .unwrap();
    assert!(
        layout.row_for_offset(heading.offset) > caption_row,
        "heading anchor must start on its own paragraph after unseparated preceding text"
    );
}

#[test]
fn narrow_prefixed_roles_keep_source_cells_and_control_headings_stay_in_bounds() {
    let narrow_doc = parse_document(
        "narrow.html",
        "<blockquote>中文测试</blockquote><pre>中文</pre>",
    )
    .unwrap();
    let narrow_layout = layout_document(&narrow_doc, LayoutOptions::reader(4, false));
    assert!(
        narrow_layout
            .source_cells
            .iter()
            .any(|cell| cell.text == "中"),
        "unprefixed narrow fallback must preserve UTF-8 source cells"
    );
    assert!(
        narrow_layout
            .text_lines
            .iter()
            .all(|line| ers::document::text::cell_width(line) <= 4),
        "narrow fallback lines must stay within width"
    );

    let control_doc = parse_document(
        "control.html",
        "<h3>Heading&#129;with control character that must not shift right</h3>",
    )
    .unwrap();
    let width = 32;
    let control_layout = layout_document(&control_doc, LayoutOptions::reader(width, false));
    for (row, line) in control_layout.text_lines.iter().enumerate() {
        let rendered_width: usize = ers::document::text::graphemes(line)
            .into_iter()
            .map(ers::document::text::cell_width)
            .sum();
        assert!(
            rendered_width <= width,
            "row {row} rendered width {rendered_width} exceeded {width}: {line:?}"
        );
    }
}

#[test]
fn layout_fixtures_preserve_visible_rows() {
    let fixtures =
        serde_json::from_str::<Vec<LayoutFixture>>(include_str!("fixtures/layouts.json")).unwrap();
    for fixture in fixtures {
        let document = parse_document("chapter.xhtml", &fixture.html).unwrap();
        let layout = layout_document(
            &document,
            LayoutOptions {
                width: fixture.width,
                starting_row: fixture.starting_row,
                image_rows: 1,
            },
        );
        assert_eq!(
            layout.text_lines.iter().cloned().collect::<Vec<_>>(),
            fixture.text_lines,
            "{}",
            fixture.name
        );
        assert_eq!(layout.image_maps, fixture.image_maps, "{}", fixture.name);
        assert_eq!(
            layout.source_cells, fixture.source_cells,
            "{}",
            fixture.name
        );
        assert_eq!(layout.formatting, fixture.formatting, "{}", fixture.name);
        assert_eq!(document.semantics.links, fixture.links, "{}", fixture.name);
        assert_eq!(
            document.semantics.anchors, fixture.anchors,
            "{}",
            fixture.name
        );
    }
}
