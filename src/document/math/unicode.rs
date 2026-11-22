use super::symbols::{
    accent_combining, apply_math_font, delimiter_symbol, tex_symbol, to_subscript_str,
    to_superscript_str,
};
use crate::document::text::{cell_width, wrap_text};
use ratex_parser::parse_node::AtomFamily;
use ratex_parser::{ParseNode, parse};

pub(super) fn render_inline_unicode(latex: &str) -> String {
    let trimmed = latex.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    match parse(trimmed) {
        Ok(nodes) => collapse_inline_spaces(&render_inline_nodes(&nodes)),
        Err(_) => fallback_unicode(trimmed),
    }
}

pub(super) fn render_display_unicode(latex: &str, max_width: usize) -> Vec<String> {
    let trimmed = latex.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let width = max_width.max(1);
    if let Ok(nodes) = parse(trimmed) {
        let boxed = render_box_nodes(&nodes);
        let lines = trim_common_indent(boxed.lines);
        if !lines.is_empty() && lines.iter().all(|line| cell_width(line) <= width) {
            return lines;
        }
    }
    let inline = render_inline_unicode(trimmed);
    wrap_text(&inline, width)
}

fn render_inline_nodes(nodes: &[ParseNode]) -> String {
    render_inline_nodes_with_spacing(nodes, true)
}

fn render_script_node(node: &ParseNode) -> String {
    match node {
        ParseNode::OrdGroup { body, .. } => render_inline_nodes_with_spacing(body, false),
        _ => render_inline_node(node),
    }
}

fn render_inline_nodes_with_spacing(nodes: &[ParseNode], bin_spaces: bool) -> String {
    let mut out = String::new();
    for (index, node) in nodes.iter().enumerate() {
        let piece = render_inline_node(node);
        if piece.is_empty() {
            continue;
        }
        let prev_is_open = out.ends_with(['(', '[', '{', '⟨', '⌊', '⌈']);
        if bin_spaces && needs_space_before(node, index > 0 && !out.ends_with(' ') && !prev_is_open)
        {
            out.push(' ');
        }
        out.push_str(&piece);
        if bin_spaces && needs_space_after(node) && !out.ends_with(' ') {
            out.push(' ');
        }
    }
    out
}

fn needs_space_before(node: &ParseNode, has_prev: bool) -> bool {
    if !has_prev {
        return false;
    }
    matches!(
        node,
        ParseNode::Atom {
            family: AtomFamily::Bin | AtomFamily::Rel,
            ..
        }
    )
}

fn needs_space_after(node: &ParseNode) -> bool {
    matches!(
        node,
        ParseNode::Atom {
            family: AtomFamily::Bin | AtomFamily::Rel | AtomFamily::Punct,
            ..
        }
    )
}

fn render_inline_node(node: &ParseNode) -> String {
    match node {
        ParseNode::Atom { text, .. } => map_token(text),
        ParseNode::MathOrd { text, .. }
        | ParseNode::TextOrd { text, .. }
        | ParseNode::OpToken { text, .. }
        | ParseNode::AccentToken { text, .. } => map_token(text),
        ParseNode::SpacingNode { text, .. } => tex_symbol(text).unwrap_or(" ").to_owned(),
        ParseNode::OrdGroup { body, .. }
        | ParseNode::Color { body, .. }
        | ParseNode::Styling { body, .. }
        | ParseNode::Sizing { body, .. }
        | ParseNode::MClass { body, .. }
        | ParseNode::HBox { body, .. }
        | ParseNode::Pmb { body, .. }
        | ParseNode::Href { body, .. }
        | ParseNode::Html { body, .. } => render_inline_nodes(body),
        ParseNode::Smash { body, .. }
        | ParseNode::RaiseBox { body, .. }
        | ParseNode::VCenter { body, .. }
        | ParseNode::Enclose { body, .. }
        | ParseNode::Lap { body, .. } => render_inline_node(body),
        ParseNode::Text { body, .. } => render_text_nodes(body),
        ParseNode::Font { font, body, .. } => apply_math_font(font, &render_inline_node(body)),
        ParseNode::SupSub { base, sup, sub, .. } => {
            let mut out = base.as_deref().map(render_inline_node).unwrap_or_default();
            if let Some(sub) = sub {
                out.push_str(&format_subscript(&render_script_node(sub)));
            }
            if let Some(sup) = sup {
                out.push_str(&format_superscript(&render_script_node(sup)));
            }
            out
        }
        ParseNode::GenFrac {
            numer,
            denom,
            has_bar_line,
            left_delim,
            right_delim,
            ..
        } => {
            let top = collapse_inline_spaces(&render_inline_node(numer));
            let bottom = collapse_inline_spaces(&render_inline_node(denom));
            let left = left_delim.as_deref().map(delimiter_symbol).unwrap_or("");
            let right = right_delim.as_deref().map(delimiter_symbol).unwrap_or("");
            if *has_bar_line {
                format!("{left}{}/{}{right}", wrap_atom(&top), wrap_atom(&bottom))
            } else {
                format!("{left}{top} choose {bottom}{right}")
            }
        }
        ParseNode::Sqrt { body, index, .. } => {
            let radicand = collapse_inline_spaces(&render_inline_node(body));
            let prefix = index
                .as_deref()
                .map(|idx| format_superscript(&render_inline_node(idx)))
                .unwrap_or_default();
            format!("{prefix}√{}", wrap_atom(&radicand))
        }
        ParseNode::Accent { label, base, .. } => apply_accent(label, &render_inline_node(base)),
        ParseNode::AccentUnder { base, .. } | ParseNode::Underline { body: base, .. } => {
            render_inline_node(base)
        }
        ParseNode::Overline { body, .. } => apply_accent(r"\overline", &render_inline_node(body)),
        ParseNode::Op { name, body, .. } => {
            if let Some(body) = body {
                render_inline_nodes(body)
            } else if let Some(name) = name {
                map_operator_name(name)
            } else {
                String::new()
            }
        }
        ParseNode::OperatorName { body, .. } => render_text_nodes(body),
        ParseNode::LeftRight {
            body, left, right, ..
        } => format!(
            "{}{}{}",
            delimiter_symbol(left),
            render_inline_nodes(body),
            delimiter_symbol(right)
        ),
        ParseNode::Middle { delim, .. } | ParseNode::DelimSizing { delim, .. } => {
            delimiter_symbol(delim).to_owned()
        }
        ParseNode::Array { body, .. } => body
            .iter()
            .map(|row| {
                row.iter()
                    .map(render_inline_node)
                    .filter(|cell| !cell.is_empty())
                    .collect::<Vec<_>>()
                    .join("  ")
            })
            .filter(|row| !row.is_empty())
            .collect::<Vec<_>>()
            .join("; "),
        ParseNode::Cr { .. } => "; ".into(),
        ParseNode::Kern { .. } => " ".into(),
        ParseNode::Verb { body, .. } => body.clone(),
        ParseNode::Raw { string, .. } => string.clone(),
        ParseNode::Tag { body, tag, .. } => {
            format!(
                "{} ({})",
                render_inline_nodes(body),
                render_inline_nodes(tag)
            )
        }
        ParseNode::HorizBrace { base, .. } => render_inline_node(base),
        ParseNode::XArrow { label, body, .. } => {
            let arrow = tex_symbol(label).unwrap_or("→");
            let above = render_inline_node(body);
            if above.is_empty() {
                format!(" {arrow} ")
            } else {
                format!(" {arrow}({above}) ")
            }
        }
        ParseNode::MathChoice { text, display, .. } => {
            if !text.is_empty() {
                render_inline_nodes(text)
            } else {
                render_inline_nodes(display)
            }
        }
        ParseNode::HtmlMathMl { html, .. } => render_inline_nodes(html),
        _ => String::new(),
    }
}

fn render_text_nodes(nodes: &[ParseNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            ParseNode::TextOrd { text, .. }
            | ParseNode::MathOrd { text, .. }
            | ParseNode::Atom { text, .. } => out.push_str(text),
            ParseNode::SpacingNode { .. } => out.push(' '),
            ParseNode::OrdGroup { body, .. } | ParseNode::Text { body, .. } => {
                out.push_str(&render_text_nodes(body))
            }
            _ => out.push_str(&render_inline_node(node)),
        }
    }
    out
}

fn map_token(token: &str) -> String {
    if token == "-" {
        return "−".into();
    }
    if let Some(symbol) = tex_symbol(token) {
        return symbol.to_owned();
    }
    if let Some(stripped) = token.strip_prefix('\\') {
        return stripped.to_owned();
    }
    token.to_owned()
}

fn map_operator_name(name: &str) -> String {
    if let Some(symbol) = tex_symbol(name) {
        return symbol.to_owned();
    }
    name.strip_prefix('\\').unwrap_or(name).to_owned()
}

fn apply_accent(label: &str, base: &str) -> String {
    let trimmed = base.trim();
    if let Some(combining) = accent_combining(label)
        && trimmed.chars().count() == 1
    {
        return format!("{trimmed}{combining}");
    }
    match label {
        r"\overline" | r"\bar" => format!("‾({trimmed})"),
        r"\hat" | r"\widehat" => format!("^{trimmed}"),
        r"\tilde" | r"\widetilde" => format!("~{trimmed}"),
        _ => trimmed.to_owned(),
    }
}

fn format_superscript(raw: &str) -> String {
    let text = collapse_inline_spaces(raw);
    if text.is_empty() {
        return String::new();
    }
    if let Some(converted) = to_superscript_str(&text) {
        return converted;
    }
    if is_simple_atom(&text) {
        format!("^{text}")
    } else {
        format!("^({text})")
    }
}

fn format_subscript(raw: &str) -> String {
    let text = collapse_inline_spaces(raw);
    if text.is_empty() {
        return String::new();
    }
    if let Some(converted) = to_subscript_str(&text) {
        return converted;
    }
    if is_simple_atom(&text) {
        format!("_{text}")
    } else {
        format!("_({text})")
    }
}

fn is_simple_atom(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return true;
    };
    if chars.next().is_none() {
        return !first.is_whitespace();
    }
    text.chars().all(|ch| ch.is_alphanumeric())
}

fn wrap_atom(text: &str) -> String {
    if is_simple_atom(text) {
        text.to_owned()
    } else {
        format!("({text})")
    }
}

fn collapse_inline_spaces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !prev_space && !out.is_empty() {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out.trim().to_owned()
}

fn fallback_unicode(latex: &str) -> String {
    let mut out = String::with_capacity(latex.len());
    let mut chars = latex.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            let mut cmd = String::from("\\");
            if let Some(&next) = chars.peek() {
                if next.is_ascii_alphabetic() {
                    while let Some(&c) = chars.peek() {
                        if c.is_ascii_alphabetic() {
                            cmd.push(c);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                } else {
                    cmd.push(next);
                    chars.next();
                }
            }
            if let Some(sym) = tex_symbol(&cmd) {
                out.push_str(sym);
            } else if !matches!(
                cmd.as_str(),
                r"\left" | r"\right" | r"\text" | r"\mathrm" | r"\mathbf" | r"\displaystyle"
            ) {
                out.push_str(cmd.strip_prefix('\\').unwrap_or(&cmd));
            }
        } else if ch != '{' && ch != '}' {
            out.push(ch);
        }
    }
    collapse_inline_spaces(&out)
}

// --- 2D Character-Grid Layout (MathBox) for Display Math ---

#[derive(Clone, Debug)]
struct MathBox {
    lines: Vec<String>,
    baseline: usize,
    width: usize,
}

impl MathBox {
    fn empty() -> Self {
        Self {
            lines: vec![String::new()],
            baseline: 0,
            width: 0,
        }
    }

    fn text(value: String) -> Self {
        let width = cell_width(&value);
        Self {
            lines: vec![value],
            baseline: 0,
            width,
        }
    }

    fn height(&self) -> usize {
        self.lines.len()
    }

    fn depth(&self) -> usize {
        self.lines.len().saturating_sub(self.baseline + 1)
    }

    fn concat_horizontal(boxes: Vec<Self>) -> Self {
        let non_empty: Vec<Self> = boxes
            .into_iter()
            .filter(|b| b.width > 0 || b.height() > 1)
            .collect();
        if non_empty.is_empty() {
            return Self::empty();
        }
        let max_ascent = non_empty.iter().map(|b| b.baseline).max().unwrap_or(0);
        let max_descent = non_empty.iter().map(Self::depth).max().unwrap_or(0);
        let total_height = max_ascent + 1 + max_descent;
        let total_width: usize = non_empty.iter().map(|b| b.width).sum();
        let mut lines = vec![String::with_capacity(total_width); total_height];
        for b in non_empty {
            let pad_top = max_ascent - b.baseline;
            for (row, line) in lines.iter_mut().enumerate() {
                if row >= pad_top && row < pad_top + b.height() {
                    let content = &b.lines[row - pad_top];
                    line.push_str(content);
                    let w = cell_width(content);
                    if w < b.width {
                        line.push_str(&" ".repeat(b.width - w));
                    }
                } else {
                    line.push_str(&" ".repeat(b.width));
                }
            }
        }
        Self {
            lines,
            baseline: max_ascent,
            width: total_width,
        }
    }

    fn stack_centered(top: Self, middle: Option<String>, bottom: Self) -> Self {
        let mid_width = middle.as_deref().map_or(0, cell_width);
        let width = top.width.max(bottom.width).max(mid_width);
        let mut lines = Vec::new();
        for line in top.lines {
            lines.push(pad_center(&line, width));
        }
        let baseline = lines.len();
        if let Some(mid) = middle {
            lines.push(pad_center(&mid, width));
        }
        for line in bottom.lines {
            lines.push(pad_center(&line, width));
        }
        let max_row = lines.len().saturating_sub(1);
        Self {
            lines,
            baseline: baseline.min(max_row),
            width,
        }
    }
}

fn pad_center(text: &str, width: usize) -> String {
    let w = cell_width(text);
    if w >= width {
        return text.to_owned();
    }
    let left = (width - w) / 2;
    let right = width - w - left;
    format!("{}{}{}", " ".repeat(left), text, " ".repeat(right))
}

fn render_box_nodes(nodes: &[ParseNode]) -> MathBox {
    let mut parts: Vec<MathBox> = Vec::new();
    let mut prev_is_space = true;
    for (index, node) in nodes.iter().enumerate() {
        let boxed = render_box_node(node);
        if boxed.width == 0 && boxed.height() == 1 {
            continue;
        }
        let is_space = boxed.height() == 1 && boxed.lines[0].chars().all(char::is_whitespace);
        let is_colon_eq = matches!(node, ParseNode::Atom { text, .. } if text == "=")
            && index > 0
            && matches!(nodes.get(index - 1), Some(ParseNode::Atom { text, .. }) if text == ":");
        if is_colon_eq && prev_is_space {
            parts.pop();
            prev_is_space = false;
        }
        if !is_space && !is_colon_eq && needs_space_before(node, index > 0 && !prev_is_space) {
            parts.push(MathBox::text(" ".into()));
        }
        parts.push(boxed);
        prev_is_space = is_space;
        if !is_space && needs_space_after(node) {
            parts.push(MathBox::text(" ".into()));
            prev_is_space = true;
        }
    }
    MathBox::concat_horizontal(parts)
}

fn trim_common_indent(lines: Vec<String>) -> Vec<String> {
    let trimmed: Vec<String> = lines
        .into_iter()
        .map(|line| line.trim_end().to_owned())
        .collect();
    let indent = trimmed
        .iter()
        .filter(|line| !line.is_empty())
        .map(|line| line.bytes().take_while(|byte| *byte == b' ').count())
        .min()
        .unwrap_or(0);
    if indent == 0 {
        return trimmed;
    }
    trimmed
        .into_iter()
        .map(|line| {
            if line.len() >= indent {
                line[indent..].to_owned()
            } else {
                line
            }
        })
        .collect()
}

fn render_box_node(node: &ParseNode) -> MathBox {
    match node {
        ParseNode::GenFrac {
            numer,
            denom,
            has_bar_line,
            left_delim,
            right_delim,
            ..
        } => {
            let top = render_box_node(numer);
            let bottom = render_box_node(denom);
            let bar_width = top.width.max(bottom.width).max(1) + 2;
            let bar = if *has_bar_line {
                Some("─".repeat(bar_width))
            } else {
                None
            };
            let frac = MathBox::stack_centered(top, bar, bottom);
            wrap_with_delims(
                frac,
                left_delim.as_deref().unwrap_or(""),
                right_delim.as_deref().unwrap_or(""),
            )
        }
        ParseNode::Sqrt { body, index, .. } => {
            let inner = render_box_node(body);
            let prefix = index
                .as_deref()
                .map(|idx| format_superscript(&render_inline_node(idx)))
                .unwrap_or_default();
            if inner.height() == 1 {
                let overbar = "─".repeat(inner.width);
                let top_line = format!("{}{}", " ".repeat(cell_width(&prefix) + 1), overbar);
                let bot_line = format!("{prefix}√{}", inner.lines[0]);
                let width = cell_width(&top_line).max(cell_width(&bot_line));
                MathBox {
                    lines: vec![top_line, bot_line],
                    baseline: 1,
                    width,
                }
            } else {
                let mut lines = Vec::with_capacity(inner.height() + 1);
                lines.push(format!(" ┌{}", "─".repeat(inner.width)));
                for (i, line) in inner.lines.iter().enumerate() {
                    let stem = if i + 1 == inner.height() {
                        "√"
                    } else {
                        " │"
                    };
                    if i + 1 == inner.height() {
                        lines.push(format!("{stem} {line}"));
                    } else {
                        lines.push(format!("{stem}{line}"));
                    }
                }
                let width = lines.iter().map(|l| cell_width(l)).max().unwrap_or(0);
                MathBox {
                    lines,
                    baseline: inner.baseline + 1,
                    width,
                }
            }
        }
        ParseNode::SupSub { base, sup, sub, .. } => {
            // Stack limits above/below big operators (\sum, \prod, \int) in display mode.
            if let Some(ParseNode::Op {
                limits: true,
                name: Some(name),
                ..
            }) = base.as_deref()
            {
                let op_sym = map_operator_name(name);
                let top = sup
                    .as_deref()
                    .map(|n| MathBox::text(collapse_inline_spaces(&render_inline_node(n))))
                    .unwrap_or_else(MathBox::empty);
                let bottom = sub
                    .as_deref()
                    .map(|n| MathBox::text(collapse_inline_spaces(&render_inline_node(n))))
                    .unwrap_or_else(MathBox::empty);
                if top.width > 0 || bottom.width > 0 {
                    let width = top.width.max(bottom.width).max(cell_width(&op_sym));
                    let mut lines = Vec::new();
                    if top.width > 0 {
                        lines.push(pad_center(&top.lines[0], width));
                    }
                    let baseline = lines.len();
                    lines.push(pad_center(&op_sym, width));
                    if bottom.width > 0 {
                        lines.push(pad_center(&bottom.lines[0], width));
                    }
                    return MathBox {
                        lines,
                        baseline,
                        width,
                    };
                }
            }
            let base_box = base
                .as_deref()
                .map(render_box_node)
                .unwrap_or_else(MathBox::empty);
            let mut scripts = String::new();
            if let Some(sub) = sub {
                scripts.push_str(&format_subscript(&render_script_node(sub)));
            }
            if let Some(sup) = sup {
                scripts.push_str(&format_superscript(&render_script_node(sup)));
            }
            if scripts.is_empty() {
                base_box
            } else {
                MathBox::concat_horizontal(vec![base_box, MathBox::text(scripts)])
            }
        }
        ParseNode::LeftRight {
            body, left, right, ..
        } => {
            let inner = render_box_nodes(body);
            wrap_with_delims(inner, left, right)
        }
        ParseNode::Array { body, .. } => render_array_box(body),
        ParseNode::OrdGroup { body, .. }
        | ParseNode::Color { body, .. }
        | ParseNode::Styling { body, .. }
        | ParseNode::Sizing { body, .. }
        | ParseNode::MClass { body, .. }
        | ParseNode::HBox { body, .. }
        | ParseNode::Pmb { body, .. }
        | ParseNode::Href { body, .. }
        | ParseNode::Html { body, .. } => render_box_nodes(body),
        ParseNode::Smash { body, .. }
        | ParseNode::RaiseBox { body, .. }
        | ParseNode::VCenter { body, .. }
        | ParseNode::Enclose { body, .. }
        | ParseNode::Lap { body, .. } => render_box_node(body),
        ParseNode::MathChoice { display, text, .. } => {
            if !display.is_empty() {
                render_box_nodes(display)
            } else {
                render_box_nodes(text)
            }
        }
        ParseNode::SpacingNode { text, .. } => {
            let space = match text.as_str() {
                r"\qquad" => "    ",
                r"\quad" => "  ",
                _ => tex_symbol(text).unwrap_or(" "),
            };
            MathBox::text(space.to_owned())
        }
        _ => MathBox::text(render_inline_node(node)),
    }
}

fn wrap_with_delims(inner: MathBox, left: &str, right: &str) -> MathBox {
    let l = delimiter_symbol(left);
    let r = delimiter_symbol(right);
    if l.is_empty() && r.is_empty() {
        return inner;
    }
    let h = inner.height();
    let baseline = inner.baseline;
    let mut parts = Vec::new();
    if !l.is_empty() {
        parts.push(tall_delimiter(l, h, baseline));
    }
    parts.push(inner);
    if !r.is_empty() {
        parts.push(tall_delimiter(r, h, baseline));
    }
    MathBox::concat_horizontal(parts)
}

fn tall_delimiter(sym: &str, height: usize, baseline: usize) -> MathBox {
    if height <= 1 {
        return MathBox::text(sym.to_owned());
    }
    let (top, mid, bot) = match sym {
        "(" => ("⎛", "⎜", "⎝"),
        ")" => ("⎞", "⎟", "⎠"),
        "[" => ("⎡", "⎢", "⎣"),
        "]" => ("⎤", "⎥", "⎦"),
        "{" => ("⎧", "⎨", "⎩"),
        "}" => ("⎫", "⎬", "⎭"),
        "|" | "∣" => ("│", "│", "│"),
        "‖" => ("║", "║", "║"),
        _ => (sym, sym, sym),
    };
    let mut lines = Vec::with_capacity(height);
    for i in 0..height {
        let ch = if i == 0 {
            top
        } else if i + 1 == height {
            bot
        } else {
            mid
        };
        lines.push(ch.to_owned());
    }
    let width = lines.iter().map(|l| cell_width(l)).max().unwrap_or(1);
    MathBox {
        lines,
        baseline: baseline.min(height - 1),
        width,
    }
}

fn render_array_box(rows: &[Vec<ParseNode>]) -> MathBox {
    if rows.is_empty() {
        return MathBox::empty();
    }
    let rendered_rows: Vec<Vec<MathBox>> = rows
        .iter()
        .map(|row| row.iter().map(render_box_node).collect())
        .collect();
    let cols = rendered_rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut col_widths = vec![0usize; cols];
    for row in &rendered_rows {
        for (c, cell) in row.iter().enumerate() {
            col_widths[c] = col_widths[c].max(cell.width);
        }
    }
    let mut all_lines = Vec::new();
    for row in rendered_rows {
        let mut padded_cells = Vec::new();
        for (c, cell) in row.into_iter().enumerate() {
            let target = col_widths[c];
            let pad = target.saturating_sub(cell.width);
            if pad > 0 {
                padded_cells.push(MathBox::concat_horizontal(vec![
                    cell,
                    MathBox::text(" ".repeat(pad)),
                ]));
            } else {
                padded_cells.push(cell);
            }
            if c + 1 < cols {
                padded_cells.push(MathBox::text("  ".into()));
            }
        }
        let row_box = MathBox::concat_horizontal(padded_cells);
        all_lines.extend(row_box.lines);
    }
    let width = all_lines.iter().map(|l| cell_width(l)).max().unwrap_or(0);
    let baseline = all_lines.len() / 2;
    MathBox {
        lines: all_lines,
        baseline,
        width,
    }
}
