use ers::document::layout::{FontStyle, LayoutOptions, layout_document};
use ers::document::text::{ParagraphRole, cell_width, parse_document};

#[test]
fn arxiv_booktabs_table_renders_aligned_columns_with_inline_math_and_bold() {
    let html = r#"
<figure class="ltx_table">
  <figcaption>Table 1: Good views align across modalities.</figcaption>
  <table id="S3.T1.10" class="ltx_tabular ltx_centering ltx_guessed_headers">
    <tbody class="ltx_tbody">
      <tr class="ltx_tr">
        <th class="ltx_td ltx_align_left ltx_th ltx_border_tt">Text encoder</th>
        <td class="ltx_td ltx_align_center ltx_border_tt">mutual 10-NN</td>
        <td class="ltx_td ltx_align_center ltx_border_tt">R@1</td>
        <td class="ltx_td ltx_align_center ltx_border_tt">R@5</td>
      </tr>
      <tr class="ltx_tr">
        <th class="ltx_td ltx_align_left ltx_th ltx_border_t">Good-view JEPA</th>
        <td class="ltx_td ltx_align_center ltx_border_t"><span class="ltx_text ltx_font_bold">.062<math alttext="\pm" display="inline"><semantics><mo>±</mo><annotation encoding="application/x-tex">\pm</annotation></semantics></math></span><span>.003</span></td>
        <td class="ltx_td ltx_align_center ltx_border_t"><span class="ltx_text ltx_font_bold">.044</span></td>
        <td class="ltx_td ltx_align_center ltx_border_t"><span class="ltx_text ltx_font_bold">.133</span></td>
      </tr>
      <tr class="ltx_tr">
        <th class="ltx_td ltx_align_left ltx_th ltx_border_t">Random init</th>
        <td class="ltx_td ltx_align_center ltx_border_t">.010</td>
        <td class="ltx_td ltx_align_center ltx_border_t">.001</td>
        <td class="ltx_td ltx_align_center ltx_border_t">.011</td>
      </tr>
    </tbody>
  </table>
</figure>
"#;
    let doc = parse_document("arxiv.html", html).unwrap();
    let table_paragraphs: Vec<_> = doc
        .paragraphs
        .iter()
        .filter(|p| p.role == ParagraphRole::Table)
        .collect();
    assert_eq!(table_paragraphs.len(), 1);
    let table = table_paragraphs[0].table.as_ref().unwrap();
    assert_eq!(table.rows.len(), 3);
    assert_eq!(table.rows[1].cells[1].text, ".062±.003");
    assert!(table.rows[1].cells[1].is_bold);

    let width = 72;
    let layout = layout_document(&doc, LayoutOptions::reader(width, false));
    for (row, line) in layout.text_lines.iter().enumerate() {
        assert!(
            cell_width(line) <= width,
            "row {row} exceeded width {width}: {line:?}"
        );
    }
    let joined = layout
        .text_lines
        .iter()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    assert!(joined.contains("Text encoder"), "{joined}");
    assert!(joined.contains(".062±.003"), "{joined}");
    assert!(
        !joined.contains("\\pm"),
        "raw TeX leaked into table: {joined}"
    );
    assert!(
        joined.contains('━'),
        "expected heavy top/bottom rule: {joined}"
    );
    assert!(joined.contains('┼'), "expected midrule junction: {joined}");
    assert!(
        layout
            .formatting
            .iter()
            .any(|span| span.style == FontStyle::Bold),
        "expected bold spans for header/bold cells"
    );
}

#[test]
fn colspan_and_rowspan_render_with_clean_grid_junctions() {
    let html = r#"
<table>
  <thead>
    <tr>
      <th></th>
      <th colspan="2" align="center">Metrics</th>
    </tr>
    <tr>
      <th align="left">Group</th>
      <th align="right">R@1</th>
      <th align="right">R@5</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td rowspan="2">JEPA</td>
      <td align="right">.135</td>
      <td align="right">.240</td>
    </tr>
    <tr>
      <td align="right">.273</td>
      <td align="right">.363</td>
    </tr>
  </tbody>
</table>
"#;
    let doc = parse_document("spans.html", html).unwrap();
    let layout = layout_document(&doc, LayoutOptions::reader(60, false));
    let lines: Vec<String> = layout.text_lines.iter().cloned().collect();
    for line in &lines {
        assert!(cell_width(line) <= 60, "line overflowed: {line:?}");
    }
    let joined = lines.join("\n");
    assert!(joined.contains("Metrics"), "{joined}");
    assert!(joined.contains("JEPA"), "{joined}");
    assert!(joined.contains(".273"), "{joined}");
    // Row 0 spans columns 1..=2, so the separator line below row 1 should have a column junction.
    assert!(joined.contains('┼'), "{joined}");
}

#[test]
fn equation_layout_tables_remain_display_math_not_data_tables() {
    let html = r#"
<table id="S2.E1" class="ltx_equation ltx_eqn_table">
  <tbody>
    <tr class="ltx_equation ltx_eqn_row">
      <td class="ltx_eqn_cell ltx_eqn_center_padleft"></td>
      <td class="ltx_eqn_cell ltx_align_center">
        <math alttext="E = mc^2" display="block">
          <semantics><mrow><mi>E</mi><mo>=</mo><mi>m</mi><msup><mi>c</mi><mn>2</mn></msup></mrow><annotation encoding="application/x-tex">E = mc^2</annotation></semantics>
        </math>
      </td>
      <td class="ltx_eqn_cell ltx_eqn_eqno">(1)</td>
    </tr>
  </tbody>
</table>
"#;
    let doc = parse_document("eq.html", html).unwrap();
    assert!(
        doc.paragraphs
            .iter()
            .all(|p| p.role != ParagraphRole::Table),
        "equation layout table should not become ParagraphRole::Table"
    );
    assert!(
        doc.paragraphs
            .iter()
            .any(|p| p.role == ParagraphRole::Math && p.math_latex.as_deref() == Some("E = mc^2")),
        "equation inside ltx_eqn_table should remain ParagraphRole::Math"
    );
}

#[test]
fn wide_tables_respect_width_across_all_viewport_sizes() {
    let html = r#"
<table>
  <thead>
    <tr>
      <th>Model</th><th>C1</th><th>C2</th><th>C3</th><th>C4</th>
      <th>C5</th><th>C6</th><th>C7</th><th>C8</th><th>C9</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td>DINOv2 ViT-B/14</td><td>.092</td><td>.101</td><td>.110</td><td>.115</td>
      <td>.126</td><td>.099</td><td>.102</td><td>.114</td><td>.166</td>
    </tr>
  </tbody>
</table>
"#;
    let doc = parse_document("wide.html", html).unwrap();
    for width in [4, 8, 16, 32, 48, 80] {
        let layout = layout_document(&doc, LayoutOptions::reader(width, false));
        for (row, line) in layout.text_lines.iter().enumerate() {
            let w = cell_width(line);
            assert!(
                w <= width,
                "width={width} row={row} exceeded width ({w} > {width}): {line:?}"
            );
        }
        let joined = layout
            .text_lines
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        if width >= 16 {
            assert!(
                joined.contains(".166"),
                "width={width} dropped last cell: {joined}"
            );
        }
    }
}
