use pulldown_cmark::{Event, LinkType, Options, Parser, Tag, TagEnd};

const IMAGE_EXTENSIONS: &[&str] = &[
    ".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg", ".bmp", ".ico", ".avif",
];

// #NOTE: Parallel Extract emits embedded figures as ordinary links (often with empty label) rather than Markdown images.
fn is_image_url(url: &str) -> bool {
    let path = url
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    IMAGE_EXTENSIONS
        .iter()
        .any(|extension| path.ends_with(extension))
}

fn normalize_image_links<'a>(events: impl Iterator<Item = Event<'a>>) -> Vec<Event<'a>> {
    let mut output = Vec::new();
    let mut active_image_link = false;
    for event in events {
        match event {
            Event::Start(Tag::Link {
                link_type: LinkType::Inline | LinkType::Reference | LinkType::Autolink,
                dest_url,
                title,
                id,
            }) if is_image_url(&dest_url) => {
                active_image_link = true;
                output.push(Event::Start(Tag::Image {
                    link_type: LinkType::Inline,
                    dest_url,
                    title,
                    id,
                }));
            }
            Event::End(TagEnd::Link) if active_image_link => {
                active_image_link = false;
                output.push(Event::End(TagEnd::Image));
            }
            Event::InlineMath(tex) => {
                let escaped = html_escape::encode_text(&tex);
                output.push(Event::Html(
                    format!(
                        "<math><semantics><annotation encoding=\"application/x-tex\">{escaped}</annotation></semantics></math>"
                    )
                    .into(),
                ));
            }
            Event::DisplayMath(tex) => {
                let escaped = html_escape::encode_text(&tex);
                output.push(Event::Html(
                    format!(
                        "<math display=\"block\"><semantics><annotation encoding=\"application/x-tex\">{escaped}</annotation></semantics></math>"
                    )
                    .into(),
                ));
            }

            other => output.push(other),
        }
    }
    output
}

pub(super) fn markdown_to_html(markdown: &str, title: Option<&str>) -> String {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_MATH;
    let events = normalize_image_links(Parser::new_ext(markdown, options));
    let mut body = String::with_capacity(markdown.len());
    pulldown_cmark::html::push_html(&mut body, events.into_iter());

    let mut html = String::with_capacity(body.len() + 128);
    html.push_str("<!DOCTYPE html><html><head><meta charset=\"utf-8\">");
    if let Some(title) = title.map(str::trim).filter(|title| !title.is_empty()) {
        html.push_str("<title>");
        html.push_str(&html_escape::encode_text(title));
        html.push_str("</title>");
    }
    html.push_str("</head><body>");
    html.push_str(&body);
    html.push_str("</body></html>");
    html
}

#[cfg(test)]
mod tests {
    use super::markdown_to_html;
    use crate::document::layout::{LayoutOptions, layout_document};
    use crate::document::text::parse_document;

    #[test]
    fn converts_markdown_headings_tables_and_image_links() {
        let markdown = "\
#### 3\\.B\\.2 Long heading with escaped dots

[](https://example.com/chart.svg)

[Caption](https://example.com/figure.webp)

Regular [article link](https://example.com/post).

| Col A | Col B |
| --- | --- |
| one | two |
";
        let html = markdown_to_html(markdown, Some("METR <Update>"));
        assert!(html.contains("<title>METR &lt;Update&gt;</title>"));
        assert!(html.contains("<img src=\"https://example.com/chart.svg\""));
        assert!(html.contains("<img src=\"https://example.com/figure.webp\""));
        assert!(html.contains("<a href=\"https://example.com/post\">article link</a>"));

        let parsed = parse_document("_", &html).unwrap();
        let images = parsed
            .paragraphs
            .iter()
            .filter_map(|paragraph| paragraph.image_path.as_deref())
            .collect::<Vec<_>>();
        assert_eq!(
            images,
            [
                "https://example.com/chart.svg",
                "https://example.com/figure.webp"
            ]
        );
        assert!(
            parsed
                .paragraphs
                .iter()
                .any(|paragraph| paragraph.text == "3.B.2 Long heading with escaped dots")
        );
        let layout = layout_document(&parsed, LayoutOptions::reader(32, false));
        assert!(
            layout
                .text_lines
                .iter()
                .all(|line| crate::document::text::cell_width(line) <= 32)
        );
    }

    #[test]
    fn converts_markdown_inline_and_display_math() {
        let markdown =
            "Inline $\\alpha + \\beta$ here.\n\n$$\n\\frac{1}{r_{\\mathrm{cog}}} - 1\n$$\n";
        let html = markdown_to_html(markdown, None);
        let parsed = parse_document("_", &html).unwrap();
        assert!(
            parsed.source.text.contains("Inline α + β here."),
            "{}",
            parsed.source.text
        );
        assert!(parsed.paragraphs.iter().any(|p| {
            p.role == crate::document::text::ParagraphRole::Math
                && p.math_latex
                    .as_deref()
                    .is_some_and(|tex| tex.contains(r"\frac{1}"))
        }));
    }
}
