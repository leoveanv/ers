pub(crate) mod html;
pub mod layout;
pub mod links;
pub mod text;

use std::collections::BTreeMap;

use layout::{LayoutOptions, TextLayout, layout_document};
use text::{ParsedDocument, python_whitespace};

pub(crate) const NORMALIZATION_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceDocument {
    pub content_id: String,
    pub text: String,
    pub normalization_version: u32,
}

impl SourceDocument {
    pub fn new(content_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            content_id: content_id.into(),
            text: text.into(),
            normalization_version: NORMALIZATION_VERSION,
        }
    }
}

#[derive(Clone, Debug)]
struct RowFormatting {
    spans: Vec<crate::document::layout::StyleSpan>,
}

#[derive(Clone, Debug)]
pub struct RenderedDocument {
    pub index: usize,
    pub width: usize,
    pub parsed: ParsedDocument,
    pub layout: TextLayout,
    letter_prefix: Vec<usize>,
    formatting_rows: BTreeMap<usize, RowFormatting>,
}

impl RenderedDocument {
    pub fn new(index: usize, parsed: ParsedDocument, width: usize) -> Self {
        Self::with_layout(
            index,
            parsed,
            LayoutOptions {
                width,
                starting_row: 0,
                image_rows: 1,
            },
        )
    }

    pub fn with_layout(index: usize, parsed: ParsedDocument, options: LayoutOptions) -> Self {
        let width = options.width;
        let layout = layout_document(&parsed, options);
        let mut letter_prefix = vec![0];
        let mut count = 0;
        for character in parsed.source.text.chars() {
            count += usize::from(!python_whitespace(character));
            letter_prefix.push(count);
        }
        let mut formatting_rows = BTreeMap::<usize, RowFormatting>::new();
        for span in &layout.formatting {
            formatting_rows
                .entry(span.row)
                .or_insert_with(|| RowFormatting { spans: Vec::new() })
                .spans
                .push(span.clone());
        }
        Self {
            index,
            parsed,
            layout,
            width,
            letter_prefix,
            formatting_rows,
        }
    }

    pub fn styles_on_row(&self, row: usize) -> &[crate::document::layout::StyleSpan] {
        self.formatting_rows
            .get(&row)
            .map_or(&[], |row| row.spans.as_slice())
    }

    pub fn letters_before(&self, offset: usize) -> usize {
        self.letter_prefix
            .get(offset)
            .copied()
            .unwrap_or_else(|| self.letter_prefix.last().copied().unwrap_or(0))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceHeading {
    pub offset: usize,
    pub title: String,
    pub level: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferencePage {
    pub offset: usize,
    pub label: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DocumentReferences {
    pub headings: Vec<ReferenceHeading>,
    pub pages: Vec<ReferencePage>,
}

pub fn clean_page_label(label: &str) -> Option<String> {
    let label = label
        .split(crate::document::text::python_whitespace)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let label = if label
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("page "))
    {
        &label[5..]
    } else {
        &label
    };
    (!label.is_empty()).then(|| label.into())
}
