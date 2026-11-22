mod references;
mod wrapping;
use crate::document::DocumentReferences;
use references::ReferenceMetadata;

pub use wrapping::{cell_width, clip_text, graphemes, wrap_text};

use percent_encoding::percent_decode_str;

use crate::document::SourceDocument;
use crate::document::html::unescape_html;
use crate::document::html::{HtmlTag, HtmlToken, HtmlTokenizer};
use crate::document::layout::{FontStyle, ParagraphStyle};
use crate::document::links::{CharacterPosition, DocumentSemantics, LinkMetadata, TextMark};
use crate::document::math::{
    format_html_subscript, format_html_superscript, is_standalone_display_candidate,
    render_inline_unicode,
};
use crate::document::table::{CellAlign, TableCell, TableData, TableRow};
use crate::{Error, Result};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ParagraphRole {
    #[default]
    Body,
    Heading,
    Indented,
    Bullet,
    Preformatted,
    Image,
    Math,
    Table,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paragraph {
    pub text: String,
    pub raw_text: String,
    pub source_start: usize,
    pub role: ParagraphRole,
    pub image_path: Option<String>,
    pub math_latex: Option<String>,
    pub table: Option<TableData>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedDocument {
    pub source: SourceDocument,
    pub paragraphs: Vec<Paragraph>,
    pub semantics: DocumentSemantics,
    pub references: DocumentReferences,
    pub(crate) styles: Vec<ParagraphStyle>,
    pub(crate) suppressed_images: Vec<usize>,
}

#[derive(Default)]
struct ParagraphFlags {
    heading: bool,
    indented: bool,
    bullet: bool,
    preformatted: bool,
    math: bool,
    table: bool,
}

#[derive(Default)]
struct RawParagraph {
    text: String,
    flags: ParagraphFlags,
    image_path: Option<String>,
    math_latex: Option<String>,
    table: Option<TableData>,
    has_prose: bool,
    suppress_image: bool,
}

#[derive(PartialEq, Eq)]
enum ImagePlatform {
    Epub,
    Amazon,
}

// #NOTE: Adjacent EPUB/Amazon variants of the same asset retain both source markers but share one displayed image.
struct ImageVariant {
    group: String,
    platform: ImagePlatform,
    path: String,
}

impl ImageVariant {
    fn from_tag(tag: &HtmlTag, path: &str) -> Option<Self> {
        let classes = tag
            .attributes
            .iter()
            .find(|attribute| attribute.name == "class")?
            .value
            .as_deref()?;
        classes.split_whitespace().find_map(|class| {
            let (group, platform) = if let Some(group) = class.strip_suffix("-epub") {
                (group, ImagePlatform::Epub)
            } else {
                let group = class.strip_suffix("-amzn")?;
                (group, ImagePlatform::Amazon)
            };
            Some(Self {
                group: group.into(),
                platform,
                path: path.into(),
            })
        })
    }

    fn is_alternative_to(&self, previous: &Self) -> bool {
        self.group == previous.group
            && self.path == previous.path
            && self.platform != previous.platform
    }
}

impl RawParagraph {
    fn role(&self) -> ParagraphRole {
        if self.flags.heading {
            ParagraphRole::Heading
        } else if self.flags.indented {
            ParagraphRole::Indented
        } else if self.flags.bullet {
            ParagraphRole::Bullet
        } else if self.flags.preformatted {
            ParagraphRole::Preformatted
        } else if self.image_path.is_some() {
            ParagraphRole::Image
        } else if self.flags.math {
            ParagraphRole::Math
        } else if self.flags.table {
            ParagraphRole::Table
        } else {
            ParagraphRole::Body
        }
    }
}

struct MathCapture {
    display: bool,
    alttext: Option<String>,
    tex_annotation: String,
    mathml_text: String,
    depth: usize,
    annotation_depth: usize,
    tex_annotation_depth: usize,
}

struct ActiveTableCell {
    text: String,
    colspan: usize,
    rowspan: usize,
    align: CellAlign,
    is_header: bool,
    is_bold: bool,
    border_top: bool,
    border_bottom: bool,
    bold_depth: usize,
}

struct ActiveTableRow {
    cells: Vec<TableCell>,
    current_cell: Option<ActiveTableCell>,
    in_thead: bool,
    border_top: bool,
    border_bottom: bool,
}

struct TableCapture {
    is_ltx_tabular: bool,
    nested_depth: usize,
    in_thead: bool,
    in_caption: bool,
    caption: String,
    rows: Vec<TableRow>,
    current_row: Option<ActiveTableRow>,
    hidden_depth: usize,
}

impl TableCapture {
    fn new(is_ltx_tabular: bool) -> Self {
        Self {
            is_ltx_tabular,
            nested_depth: 0,
            in_thead: false,
            in_caption: false,
            caption: String::new(),
            rows: Vec::new(),
            current_row: None,
            hidden_depth: 0,
        }
    }

    fn finish_cell(&mut self) {
        if let Some(row) = self.current_row.as_mut()
            && let Some(cell) = row.current_cell.take()
        {
            row.border_top |= cell.border_top;
            row.border_bottom |= cell.border_bottom;
            row.cells.push(TableCell {
                text: cell.text.trim().to_owned(),
                colspan: cell.colspan,
                rowspan: cell.rowspan,
                align: cell.align,
                is_header: cell.is_header,
                is_bold: cell.is_bold,
                border_top: cell.border_top,
                border_bottom: cell.border_bottom,
            });
        }
    }

    fn finish_row(&mut self) {
        self.finish_cell();
        if let Some(row) = self.current_row.take()
            && !row.cells.is_empty()
        {
            self.rows.push(TableRow {
                cells: row.cells,
                in_thead: row.in_thead,
                border_top: row.border_top,
                border_bottom: row.border_bottom,
            });
        }
    }

    fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.in_caption {
            self.caption.push_str(text);
            return;
        }
        if let Some(row) = self.current_row.as_mut()
            && let Some(cell) = row.current_cell.as_mut()
        {
            if cell.text.is_empty() {
                cell.text
                    .push_str(text.trim_start_matches(python_whitespace));
            } else {
                cell.text.push_str(text);
            }
            if cell.bold_depth > 0 && text.chars().any(|c| !python_whitespace(c)) {
                cell.is_bold = true;
            }
        }
    }

    fn finish(mut self) -> (TableData, bool) {
        self.finish_row();
        let caption = self.caption.trim().to_owned();
        let data = TableData {
            caption: (!caption.is_empty()).then_some(caption),
            rows: self.rows,
        };
        let max_cols = data
            .rows
            .iter()
            .map(|row| row.cells.iter().map(|c| c.colspan.max(1)).sum::<usize>())
            .max()
            .unwrap_or(0);
        let has_header = data
            .rows
            .iter()
            .any(|row| row.in_thead || row.cells.iter().any(|c| c.is_header));
        let is_data_table = max_cols >= 2
            && (data.rows.len() >= 2
                || has_header
                || self.is_ltx_tabular
                || data.caption.is_some());
        (data, is_data_table)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScriptKind {
    Sup,
    Sub,
}

struct ActiveScript {
    kind: ScriptKind,
    paragraph_index: usize,
    byte_start: usize,
}

struct DocumentParser {
    paragraphs: Vec<RawParagraph>,
    flags: ParagraphFlags,
    hidden: bool,
    metadata: LinkMetadata,
    references: ReferenceMetadata,
    italics: Vec<TextMark>,
    bold: Vec<TextMark>,
    image_variant: Option<ImageVariant>,
    math_container_depth: usize,
    display_container_depth: usize,
    skip_depth: usize,
    script_stack: Vec<ActiveScript>,
    ltx_note_depth: usize,
    ltx_footnotetext_depth: usize,
    ltx_note_content_depth: usize,
    math_capture: Option<MathCapture>,
    table_capture: Option<TableCapture>,
}

impl DocumentParser {
    fn new() -> Self {
        Self {
            paragraphs: vec![RawParagraph::default()],
            flags: ParagraphFlags::default(),
            hidden: false,
            metadata: LinkMetadata::default(),
            references: ReferenceMetadata::default(),
            italics: Vec::new(),
            bold: Vec::new(),
            image_variant: None,
            math_container_depth: 0,
            display_container_depth: 0,
            skip_depth: 0,
            script_stack: Vec::new(),
            ltx_note_depth: 0,
            ltx_footnotetext_depth: 0,
            ltx_note_content_depth: 0,
            math_capture: None,
            table_capture: None,
        }
    }

    fn current(&mut self) -> &mut RawParagraph {
        self.paragraphs
            .last_mut()
            .expect("The parser always retains its first paragraph")
    }

    fn blank(&mut self) {
        self.paragraphs.push(RawParagraph::default());
    }

    fn position(&self) -> CharacterPosition {
        CharacterPosition {
            row: self.paragraphs.len() - 1,
            col: self
                .paragraphs
                .last()
                .map_or(0, |paragraph| paragraph.text.chars().count()),
        }
    }

    fn start(&mut self, tag: &HtmlTag) -> Result<()> {
        let opens_scope = !tag.self_closing && !is_void_html_tag(&tag.name);
        if opens_scope {
            if self.math_container_depth > 0 {
                self.math_container_depth += 1;
            } else if is_math_container(tag) {
                self.math_container_depth = 1;
            }
            if self.display_container_depth > 0 {
                self.display_container_depth += 1;
            } else if is_display_math_container(tag) {
                self.display_container_depth = 1;
            }
        }
        if self.skip_depth > 0 {
            if opens_scope {
                self.skip_depth += 1;
            }
            return Ok(());
        }
        if self.math_container_depth > 0 && is_visual_math_duplicate(tag) {
            if opens_scope {
                self.skip_depth = 1;
            }
            return Ok(());
        }
        // #NOTE: LaTeXML footnotes duplicate the mark and label inside .ltx_note_content, and footnotetext uses a dummy 0 mark.
        if self.ltx_note_content_depth > 0 && is_ltx_note_duplicate(tag) {
            if opens_scope {
                self.skip_depth = 1;
            }
            return Ok(());
        }
        if self.ltx_footnotetext_depth > 0 && has_class(tag, "ltx_note_mark") {
            if opens_scope {
                self.skip_depth = 1;
            }
            return Ok(());
        }
        if opens_scope {
            if self.ltx_note_depth > 0 {
                self.ltx_note_depth += 1;
            } else if has_class(tag, "ltx_note") {
                self.ltx_note_depth = 1;
            }
            if self.ltx_footnotetext_depth > 0 {
                self.ltx_footnotetext_depth += 1;
            } else if has_class(tag, "ltx_role_footnotetext") {
                if !self.current().text.is_empty() {
                    self.blank();
                }
                self.ltx_footnotetext_depth = 1;
            }
            if self.ltx_note_content_depth > 0 {
                self.ltx_note_content_depth += 1;
            } else if has_class(tag, "ltx_note_content") {
                if self.ltx_footnotetext_depth == 0 {
                    if !self.current().text.is_empty() && !self.current().text.ends_with(' ') {
                        self.current().text.push(' ');
                    }
                    self.current().text.push('[');
                }
                self.ltx_note_content_depth = 1;
            }
        }
        if let Some(capture) = self.math_capture.as_mut() {
            if opens_scope {
                capture.depth += 1;
                if capture.tex_annotation_depth > 0 {
                    capture.tex_annotation_depth += 1;
                } else if is_tex_annotation_tag(tag) {
                    capture.tex_annotation_depth = 1;
                }
                if capture.annotation_depth > 0 {
                    capture.annotation_depth += 1;
                } else if matches!(tag.name.as_str(), "annotation" | "annotation-xml") {
                    capture.annotation_depth = 1;
                }
            }
            return Ok(());
        }
        if !self.hidden && tag.name == "math" {
            let display = self.display_container_depth > 0
                || attr_value(tag, "display").is_some_and(|v| v.eq_ignore_ascii_case("block"));
            let alttext = attr_value(tag, "alttext")
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned);
            if opens_scope {
                self.math_capture = Some(MathCapture {
                    display,
                    alttext,
                    tex_annotation: String::new(),
                    mathml_text: String::new(),
                    depth: 1,
                    annotation_depth: 0,
                    tex_annotation_depth: 0,
                });
            } else if let Some(latex) = alttext {
                self.emit_math(Some(latex), String::new(), display);
            }
            return Ok(());
        }
        if !self.hidden
            && self.script_stack.is_empty()
            && self.table_capture.is_none()
            && tag.name == "table"
            && opens_scope
            && !is_equation_table(tag)
        {
            let position = self.position();
            self.references.start(tag, position)?;
            self.metadata.start(tag, position);
            self.table_capture = Some(TableCapture::new(has_class(tag, "ltx_tabular")));
            return Ok(());
        }
        if let Some(table) = self.table_capture.as_mut() {
            if table.hidden_depth > 0 {
                if opens_scope {
                    table.hidden_depth += 1;
                }
                return Ok(());
            }
            match tag.name.as_str() {
                "script" | "style" | "head" if opens_scope => {
                    table.hidden_depth = 1;
                }
                "table" if opens_scope => {
                    table.nested_depth += 1;
                }
                "caption" if table.nested_depth == 0 && opens_scope => {
                    table.in_caption = true;
                }
                "thead" if table.nested_depth == 0 && opens_scope => {
                    table.in_thead = true;
                }
                "tbody" | "tfoot" if table.nested_depth == 0 && opens_scope => {
                    table.in_thead = false;
                }
                "tr" if table.nested_depth == 0 && opens_scope => {
                    table.finish_row();
                    table.current_row = Some(ActiveTableRow {
                        cells: Vec::new(),
                        current_cell: None,
                        in_thead: table.in_thead,
                        border_top: has_top_border(tag),
                        border_bottom: has_bottom_border(tag),
                    });
                }
                "th" | "td" if table.nested_depth == 0 && opens_scope => {
                    if table.current_row.is_none() {
                        table.current_row = Some(ActiveTableRow {
                            cells: Vec::new(),
                            current_cell: None,
                            in_thead: table.in_thead,
                            border_top: false,
                            border_bottom: false,
                        });
                    }
                    table.finish_cell();
                    if let Some(row) = table.current_row.as_mut() {
                        let is_header = tag.name == "th";
                        row.current_cell = Some(ActiveTableCell {
                            text: String::new(),
                            colspan: parse_span_attr(tag, "colspan"),
                            rowspan: parse_span_attr(tag, "rowspan"),
                            align: parse_cell_align(tag),
                            is_header,
                            is_bold: is_header || has_class(tag, "ltx_font_bold"),
                            border_top: has_top_border(tag),
                            border_bottom: has_bottom_border(tag),
                            bold_depth: 0,
                        });
                    }
                }
                "br" => table.push_text("\n"),
                "li" if opens_scope => {
                    if let Some(row) = table.current_row.as_mut()
                        && let Some(cell) = row.current_cell.as_mut()
                    {
                        if !cell.text.is_empty() && !cell.text.ends_with('\n') {
                            cell.text.push('\n');
                        }
                        cell.text.push_str("- ");
                    }
                }
                "b" | "strong" if opens_scope => {
                    if let Some(row) = table.current_row.as_mut()
                        && let Some(cell) = row.current_cell.as_mut()
                    {
                        cell.bold_depth += 1;
                    }
                }
                "span" if opens_scope && has_class(tag, "ltx_font_bold") => {
                    if let Some(row) = table.current_row.as_mut()
                        && let Some(cell) = row.current_cell.as_mut()
                    {
                        cell.bold_depth += 1;
                    }
                }
                "p" | "div" if opens_scope => {
                    if let Some(row) = table.current_row.as_mut()
                        && let Some(cell) = row.current_cell.as_mut()
                        && !cell.text.is_empty()
                        && !cell.text.ends_with([' ', '\n'])
                    {
                        cell.text.push(' ');
                    }
                }
                _ => {}
            }
            return Ok(());
        }
        if !matches!(tag.name.as_str(), "img" | "image") {
            self.image_variant = None;
        }
        // #NOTE: Start a fresh paragraph before headings so preceding unseparated text never inherits heading layout or shifts heading anchors, except inside inline <sup>/<sub> wrappers (e.g. sr-only dialog titles).
        if !self.hidden
            && self.script_stack.is_empty()
            && is_heading(&tag.name)
            && !self.current().text.is_empty()
        {
            self.blank();
        }
        let position = self.position();
        if !self.hidden && !matches!(tag.name.as_str(), "head" | "script" | "style") {
            self.references.start(tag, position)?;
            self.metadata.start(tag, position);
            if tag.self_closing || tag.name == "img" {
                self.metadata.end(&tag.name, position);
            }
        }
        if tag.self_closing {
            if tag.name == "br" && self.script_stack.is_empty() {
                self.blank();
            } else if matches!(tag.name.as_str(), "img" | "image") {
                self.images(tag)?;
            }
            return Ok(());
        }
        if is_heading(&tag.name) {
            if self.script_stack.is_empty() {
                self.flags.heading = true;
            }
        } else {
            match tag.name.as_str() {
                "q" | "dt" | "dd" | "blockquote" => self.flags.indented = true,
                "pre" => self.flags.preformatted = true,
                "li" => self.flags.bullet = true,
                "script" | "style" | "head" => self.hidden = true,
                "sup" => {
                    let paragraph_index = self.paragraphs.len() - 1;
                    let byte_start = self.current().text.len();
                    self.script_stack.push(ActiveScript {
                        kind: ScriptKind::Sup,
                        paragraph_index,
                        byte_start,
                    });
                }
                "sub" => {
                    let paragraph_index = self.paragraphs.len() - 1;
                    let byte_start = self.current().text.len();
                    self.script_stack.push(ActiveScript {
                        kind: ScriptKind::Sub,
                        paragraph_index,
                        byte_start,
                    });
                }
                "img" | "image" => self.images(tag)?,
                "i" | "em" => start_style(&mut self.italics, position),
                "b" | "strong" => start_style(&mut self.bold, position),
                _ => {}
            }
        }
        Ok(())
    }

    fn images(&mut self, tag: &HtmlTag) -> Result<()> {
        let previous = self.image_variant.take();
        for attribute in &tag.attributes {
            if (tag.name == "img" && attribute.name == "src")
                || (tag.name == "image" && attribute.name.ends_with("href"))
            {
                let source = attribute.value.as_deref().ok_or_else(|| {
                    Error::Compatibility("An image source attribute has no value.".into())
                })?;
                let path = percent_decode_str(source).decode_utf8_lossy().into_owned();
                let variant = ImageVariant::from_tag(tag, &path);
                let suppress_image = variant.as_ref().is_some_and(|variant| {
                    previous
                        .as_ref()
                        .is_some_and(|previous| variant.is_alternative_to(previous))
                });
                self.image_variant = if suppress_image { None } else { variant };
                self.paragraphs.push(RawParagraph {
                    text: "[IMAGE]".into(),
                    image_path: Some(path),
                    math_latex: None,
                    table: None,
                    has_prose: false,
                    flags: ParagraphFlags::default(),
                    suppress_image,
                });
                // #NOTE: Always start a fresh paragraph after an image marker so HTML5 void <img> tags do not absorb following text into an unwrapped image row.
                self.blank();
            }
        }
        Ok(())
    }

    fn emit_math(&mut self, latex: Option<String>, mathml_fallback: String, display: bool) {
        self.image_variant = None;
        let canonical = latex
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        let rendered = match &canonical {
            Some(tex) => render_inline_unicode(tex),
            None => collapse_whitespace(&mathml_fallback).trim().to_owned(),
        };
        if rendered.is_empty() && canonical.is_none() {
            return;
        }
        if let Some(table) = self.table_capture.as_mut() {
            table.push_text(&rendered);
            return;
        }
        if display && let Some(tex) = canonical {
            if !self.current().text.is_empty() {
                self.blank();
            }
            self.paragraphs.push(RawParagraph {
                text: rendered,
                flags: ParagraphFlags {
                    math: true,
                    ..ParagraphFlags::default()
                },
                image_path: None,
                math_latex: Some(tex),
                table: None,
                has_prose: false,
                suppress_image: false,
            });
            self.blank();
            return;
        }
        // #NOTE: Consecutive standalone KaTeX equations inside a prose-free block become separate display math blocks.
        if let Some(tex) = canonical
            && !self.flags.heading
            && !self.flags.bullet
            && !self.flags.indented
            && !self.flags.preformatted
            && !self.current().has_prose
            && is_standalone_display_candidate(&tex)
        {
            if self.current().math_latex.is_some() {
                self.current().flags.math = true;
                self.blank();
            }
            let paragraph = self.current();
            paragraph.text = rendered;
            paragraph.math_latex = Some(tex);
            return;
        }
        let flags = ParagraphFlags {
            heading: self.flags.heading,
            bullet: !self.flags.heading && self.flags.bullet,
            indented: !self.flags.heading && !self.flags.bullet && self.flags.indented,
            preformatted: !self.flags.heading
                && !self.flags.bullet
                && !self.flags.indented
                && self.flags.preformatted,
            math: false,
            table: false,
        };
        let paragraph = self.current();
        paragraph.math_latex = None;
        paragraph.text.push_str(&rendered);
        paragraph.flags.heading |= flags.heading;
        paragraph.flags.bullet |= flags.bullet;
        paragraph.flags.indented |= flags.indented;
        paragraph.flags.preformatted |= flags.preformatted;
    }

    fn emit_table(&mut self, capture: TableCapture) {
        let (data, is_data_table) = capture.finish();
        let plain = table_plain_text(&data);
        if !is_data_table {
            if !plain.is_empty() {
                let paragraph = self.current();
                if !paragraph.text.is_empty() && !paragraph.text.ends_with(' ') {
                    paragraph.text.push(' ');
                }
                paragraph.text.push_str(&plain);
                paragraph.has_prose = true;
            }
            self.blank();
            if !self.hidden {
                self.references.end("table", &self.paragraphs);
                self.metadata.end("table", self.position());
            }
            return;
        }
        if !self.current().text.is_empty() {
            self.blank();
        }
        self.paragraphs.push(RawParagraph {
            text: plain,
            flags: ParagraphFlags {
                table: true,
                ..ParagraphFlags::default()
            },
            image_path: None,
            math_latex: None,
            table: Some(data),
            has_prose: true,
            suppress_image: false,
        });
        if !self.hidden {
            self.references.end("table", &self.paragraphs);
            self.metadata.end("table", self.position());
        }
        self.blank();
    }

    fn end(&mut self, tag: &str) {
        if self.math_container_depth > 0 {
            self.math_container_depth -= 1;
        }
        if self.display_container_depth > 0 {
            self.display_container_depth -= 1;
        }
        if self.skip_depth > 0 {
            self.skip_depth -= 1;
            return;
        }
        if self.ltx_note_content_depth > 0 {
            self.ltx_note_content_depth -= 1;
            if self.ltx_note_content_depth == 0 && self.ltx_footnotetext_depth == 0 {
                let trimmed_len = self.current().text.trim_end().len();
                self.current().text.truncate(trimmed_len);
                self.current().text.push(']');
            }
        }
        if self.ltx_footnotetext_depth > 0 {
            self.ltx_footnotetext_depth -= 1;
            if self.ltx_footnotetext_depth == 0 && !self.current().text.is_empty() {
                self.blank();
            }
        }
        if self.ltx_note_depth > 0 {
            self.ltx_note_depth -= 1;
        }
        if let Some(capture) = self.math_capture.as_mut() {
            if capture.tex_annotation_depth > 0 {
                capture.tex_annotation_depth -= 1;
            }
            if capture.annotation_depth > 0 {
                capture.annotation_depth -= 1;
            }
            capture.depth -= 1;
            if capture.depth == 0 {
                let finished = self.math_capture.take().unwrap();
                let latex = if !finished.tex_annotation.trim().is_empty() {
                    Some(finished.tex_annotation)
                } else {
                    finished.alttext
                };
                self.emit_math(latex, finished.mathml_text, finished.display);
            }
            return;
        }
        if let Some(table) = self.table_capture.as_mut() {
            if table.hidden_depth > 0 {
                table.hidden_depth -= 1;
                return;
            }
            match tag {
                "table" => {
                    if table.nested_depth > 0 {
                        table.nested_depth -= 1;
                    } else {
                        let finished = self.table_capture.take().unwrap();
                        self.emit_table(finished);
                    }
                }
                "caption" if table.nested_depth == 0 => table.in_caption = false,
                "thead" if table.nested_depth == 0 => table.in_thead = false,
                "tr" if table.nested_depth == 0 => table.finish_row(),
                "th" | "td" if table.nested_depth == 0 => table.finish_cell(),
                "b" | "strong" | "span" => {
                    if let Some(row) = table.current_row.as_mut()
                        && let Some(cell) = row.current_cell.as_mut()
                    {
                        cell.bold_depth = cell.bold_depth.saturating_sub(1);
                    }
                }
                _ => {}
            }
            return;
        }
        if !matches!(tag, "img" | "image") {
            self.image_variant = None;
        }
        if !self.hidden {
            self.references.end(tag, &self.paragraphs);
        }
        if is_heading(tag) {
            if self.script_stack.is_empty() {
                self.blank();
                self.blank();
                self.flags.heading = false;
            }
        } else {
            match tag {
                "p" | "div" | "table" => {
                    if self.script_stack.is_empty() {
                        self.blank();
                    }
                }
                "img" | "image" => {
                    if !self.current().text.is_empty() {
                        self.blank();
                    }
                }
                "script" | "style" | "head" => self.hidden = false,
                "q" | "dt" | "dd" | "blockquote" => {
                    if !self.current().text.is_empty() {
                        self.blank();
                    }
                    self.flags.indented = false;
                }
                "pre" => {
                    if !self.current().text.is_empty() {
                        self.blank();
                    }
                    self.flags.preformatted = false;
                }
                "li" => {
                    if !self.current().text.is_empty() {
                        self.blank();
                    }
                    self.flags.bullet = false;
                }
                "sub" | "sup" => {
                    if let Some(script) = self.script_stack.pop()
                        && script.paragraph_index == self.paragraphs.len() - 1
                        && script.byte_start <= self.current().text.len()
                    {
                        let inner = self.current().text.split_off(script.byte_start);
                        let formatted = match script.kind {
                            ScriptKind::Sup => format_html_superscript(&inner),
                            ScriptKind::Sub => format_html_subscript(&inner),
                        };
                        self.current().text.push_str(&formatted);
                    }
                }
                "i" | "em" => {
                    let position = self.position();
                    end_style(&mut self.italics, position);
                }
                "b" | "strong" => {
                    let position = self.position();
                    end_style(&mut self.bold, position);
                }
                _ => {}
            }
        }
        if !self.hidden {
            self.metadata.end(tag, self.position());
        }
    }

    fn data(&mut self, raw: &str) {
        if self.skip_depth > 0 {
            return;
        }
        if let Some(capture) = self.math_capture.as_mut() {
            let text = unescape_html(raw);
            if capture.tex_annotation_depth > 0 {
                capture.tex_annotation.push_str(&text);
            } else if capture.annotation_depth == 0 {
                capture.mathml_text.push_str(&text);
            }
            return;
        }
        if let Some(table) = self.table_capture.as_mut() {
            if table.hidden_depth == 0 && !raw.is_empty() {
                let text = unescape_html(&collapse_whitespace(raw));
                table.push_text(&text);
            }
            return;
        }
        let has_visible = raw.chars().any(|character| !python_whitespace(character));
        if has_visible {
            self.image_variant = None;
        }
        if raw.is_empty() || self.hidden {
            return;
        }
        if !has_visible && self.current().math_latex.is_some() && !self.current().has_prose {
            return;
        }
        let raw = if self.current().text.is_empty()
            || (self.ltx_note_content_depth > 0 && self.current().text.ends_with('['))
        {
            raw.trim_start_matches(python_whitespace)
        } else {
            raw
        };
        let line = if self.flags.preformatted {
            unescape_html(raw)
        } else {
            unescape_html(&collapse_whitespace(raw))
        };
        let flags = ParagraphFlags {
            heading: self.flags.heading,
            bullet: !self.flags.heading && self.flags.bullet,
            indented: !self.flags.heading && !self.flags.bullet && self.flags.indented,
            preformatted: !self.flags.heading
                && !self.flags.bullet
                && !self.flags.indented
                && self.flags.preformatted,
            math: false,
            table: false,
        };
        let paragraph = self.current();
        if has_visible {
            paragraph.has_prose = true;
            paragraph.math_latex = None;
        }
        paragraph.text.push_str(&line);
        paragraph.flags.heading |= flags.heading;
        paragraph.flags.bullet |= flags.bullet;
        paragraph.flags.indented |= flags.indented;
        paragraph.flags.preformatted |= flags.preformatted;
    }

    fn finish(self, content_id: &str) -> ParsedDocument {
        let mut source_start = 0;
        let mut suppressed_images = Vec::new();
        let paragraphs = self
            .paragraphs
            .into_iter()
            .enumerate()
            .map(|(index, mut raw)| {
                if raw.suppress_image {
                    suppressed_images.push(index);
                }
                if !raw.has_prose && raw.math_latex.is_some() {
                    raw.flags.math = true;
                }
                let role = raw.role();
                let text = normalize_paragraph(&raw.text);
                let paragraph = Paragraph {
                    text,
                    raw_text: raw.text,
                    source_start,
                    role,
                    image_path: raw.image_path,
                    math_latex: if role == ParagraphRole::Math {
                        raw.math_latex
                    } else {
                        None
                    },
                    table: if role == ParagraphRole::Table {
                        raw.table
                    } else {
                        None
                    },
                };
                source_start += paragraph.text.chars().count() + 1;
                paragraph
            })
            .collect::<Vec<_>>();
        let text = paragraphs
            .iter()
            .map(|paragraph| paragraph.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let source = SourceDocument::new(content_id, text);
        let semantics = self.metadata.finish(&paragraphs, &source);
        let references = self.references.finish(&paragraphs);
        let mut styles = collect_styles(&paragraphs, &self.italics, FontStyle::Italic);
        styles.extend(collect_styles(&paragraphs, &self.bold, FontStyle::Bold));
        ParsedDocument {
            source,
            paragraphs,
            semantics,
            references,
            styles,
            suppressed_images,
        }
    }
}

fn start_style(marks: &mut Vec<TextMark>, position: CharacterPosition) {
    if marks.last().is_none_or(|mark| mark.is_valid()) {
        marks.push(TextMark {
            start: position,
            end: None,
        });
    }
}

fn end_style(marks: &mut [TextMark], position: CharacterPosition) {
    if let Some(mark) = marks.last_mut() {
        mark.end = Some(position);
    }
}

fn collect_styles(
    paragraphs: &[Paragraph],
    marks: &[TextMark],
    style: FontStyle,
) -> Vec<ParagraphStyle> {
    let mut spans = Vec::new();
    for mark in marks.iter().filter(|mark| mark.is_valid()) {
        let Some(end) = mark.end else {
            continue;
        };
        for (row, paragraph) in paragraphs
            .iter()
            .enumerate()
            .take(end.row + 1)
            .skip(mark.start.row)
        {
            let start = if row == mark.start.row {
                mark.start.col
            } else {
                0
            };
            let end = if row == end.row {
                end.col
            } else {
                paragraph.raw_text.chars().count()
            };
            spans.push(ParagraphStyle {
                row,
                col: start,
                n_letters: end.saturating_sub(start),
                style,
            });
        }
    }
    spans
}

fn is_heading(tag: &str) -> bool {
    tag.len() == 2
        && tag.as_bytes().first() == Some(&b'h')
        && tag
            .as_bytes()
            .get(1)
            .is_some_and(|byte| (b'1'..=b'6').contains(byte))
}

pub(crate) fn python_whitespace(character: char) -> bool {
    character.is_whitespace() || matches!(character, '\u{001c}'..='\u{001f}')
}

fn collapse_whitespace(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut previous_whitespace = false;
    for character in value.chars() {
        let whitespace = python_whitespace(character);
        if !whitespace {
            result.push(character);
        } else if !previous_whitespace {
            result.push(' ');
        }
        previous_whitespace = whitespace;
    }
    result
}

pub fn normalize_paragraph(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut column = 0;
    for character in value.chars() {
        match character {
            '\t' => {
                let spaces = 8 - column % 8;
                result.extend(std::iter::repeat_n(' ', spaces));
                column += spaces;
            }
            '\n' => {
                result.push('\n');
                column = 0;
            }
            '\r' => {
                result.push(' ');
                column = 0;
            }
            '\u{000b}' | '\u{000c}' => {
                result.push(' ');
                column += 1;
            }
            _ => {
                result.push(character);
                column += 1;
            }
        }
    }
    result
}

fn is_void_html_tag(name: &str) -> bool {
    matches!(
        name,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

fn attr_value<'a>(tag: &'a HtmlTag, name: &str) -> Option<&'a str> {
    tag.attributes
        .iter()
        .find(|attribute| attribute.name.eq_ignore_ascii_case(name))
        .and_then(|attribute| attribute.value.as_deref())
}

fn has_class(tag: &HtmlTag, class: &str) -> bool {
    attr_value(tag, "class").is_some_and(|classes| classes.split_whitespace().any(|c| c == class))
}

fn is_math_container(tag: &HtmlTag) -> bool {
    tag.name == "mjx-container"
        || has_class(tag, "katex")
        || has_class(tag, "katex-display")
        || has_class(tag, "mwe-math-element")
}

fn is_display_math_container(tag: &HtmlTag) -> bool {
    has_class(tag, "katex-display")
        || has_class(tag, "math-display")
        || (tag.name == "mjx-container"
            && attr_value(tag, "display").is_some_and(|v| v.eq_ignore_ascii_case("true")))
}

fn is_ltx_note_duplicate(tag: &HtmlTag) -> bool {
    has_class(tag, "ltx_note_mark")
        || has_class(tag, "ltx_tag_note")
        || has_class(tag, "ltx_note_type")
}

fn is_visual_math_duplicate(tag: &HtmlTag) -> bool {
    has_class(tag, "katex-html")
        || tag.name == "mjx-math"
        || attr_value(tag, "aria-hidden").is_some_and(|v| v.eq_ignore_ascii_case("true"))
}

fn table_plain_text(data: &TableData) -> String {
    let mut parts = Vec::new();
    if let Some(caption) = data.caption.as_deref()
        && !caption.is_empty()
    {
        parts.push(caption.to_owned());
    }
    for row in &data.rows {
        let row_text = row
            .cells
            .iter()
            .map(|cell| cell.text.trim())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if !row_text.is_empty() {
            parts.push(row_text);
        }
    }
    parts.join(" ")
}

fn is_equation_table(tag: &HtmlTag) -> bool {
    has_class(tag, "ltx_eqn_table")
        || has_class(tag, "ltx_equation")
        || has_class(tag, "ltx_equationgroup")
}

fn parse_span_attr(tag: &HtmlTag, name: &str) -> usize {
    attr_value(tag, name)
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|&value| value > 0)
        .unwrap_or(1)
}

fn parse_cell_align(tag: &HtmlTag) -> CellAlign {
    if let Some(align) = attr_value(tag, "align") {
        if align.eq_ignore_ascii_case("center") {
            return CellAlign::Center;
        }
        if align.eq_ignore_ascii_case("right") {
            return CellAlign::Right;
        }
        if align.eq_ignore_ascii_case("left") {
            return CellAlign::Left;
        }
    }
    if let Some(style) = attr_value(tag, "style") {
        let lower = style.to_ascii_lowercase();
        if let Some((_, rest)) = lower.split_once("text-align:") {
            let value = rest.split(';').next().unwrap_or("").trim();
            match value {
                "center" => return CellAlign::Center,
                "right" => return CellAlign::Right,
                "left" => return CellAlign::Left,
                _ => {}
            }
        }
    }
    if has_class(tag, "ltx_align_center") || has_class(tag, "text-center") {
        return CellAlign::Center;
    }
    if has_class(tag, "ltx_align_right") || has_class(tag, "text-right") {
        return CellAlign::Right;
    }
    CellAlign::Left
}

fn has_top_border(tag: &HtmlTag) -> bool {
    has_class(tag, "ltx_border_t") || has_class(tag, "ltx_border_tt")
}

fn has_bottom_border(tag: &HtmlTag) -> bool {
    has_class(tag, "ltx_border_b") || has_class(tag, "ltx_border_bb")
}

fn is_tex_annotation_tag(tag: &HtmlTag) -> bool {
    tag.name == "annotation"
        && attr_value(tag, "encoding").is_some_and(|encoding| {
            let lower = encoding.to_ascii_lowercase();
            lower.contains("tex") || lower.contains("latex")
        })
}

// #NOTE: Canonical anchors retain tokenizer events, duplicate attributes, CR preservation, and the reader's second entity-decoding pass.
pub fn parse_document(content_id: &str, html: &str) -> Result<ParsedDocument> {
    let mut parser = DocumentParser::new();
    for token in HtmlTokenizer::new(html) {
        match token {
            HtmlToken::Start(tag) => parser.start(&tag)?,
            HtmlToken::End(tag) => parser.end(&tag),
            HtmlToken::Data(data) => parser.data(&data),
        }
    }
    Ok(parser.finish(content_id))
}
