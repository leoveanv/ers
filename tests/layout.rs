use std::collections::BTreeMap;

use ers::document::layout::{LayoutOptions, SourceCell, StyleSpan, layout_document};
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
