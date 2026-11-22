use crate::document::layout::FontStyle;
use crate::document::text::{cell_width, clip_text, wrap_text};

const MAX_COLSPAN: usize = 64;
const MAX_ROWSPAN: usize = 256;
const MIN_READABLE_COL_WIDTH: usize = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CellAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableCell {
    pub text: String,
    pub colspan: usize,
    pub rowspan: usize,
    pub align: CellAlign,
    pub is_header: bool,
    pub is_bold: bool,
    pub border_top: bool,
    pub border_bottom: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableRow {
    pub cells: Vec<TableCell>,
    pub in_thead: bool,
    pub border_top: bool,
    pub border_bottom: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableData {
    pub caption: Option<String>,
    pub rows: Vec<TableRow>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableStyleSpan {
    pub line: usize,
    pub col_chars: usize,
    pub len_chars: usize,
    pub style: FontStyle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedTable {
    pub lines: Vec<String>,
    pub styles: Vec<TableStyleSpan>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GridSlot {
    Origin {
        row_idx: usize,
        cell_idx: usize,
        colspan: usize,
    },
    Spanned,
    Covered,
    Empty,
}

struct NormalizedGrid<'a> {
    rows: &'a [TableRow],
    slots: Vec<Vec<GridSlot>>,
    ncols: usize,
    header_rows: usize,
    rule_before: Vec<bool>,
}

struct ColumnLayout {
    widths: Vec<usize>,
    padded: bool,
    left_pad: usize,
}

impl ColumnLayout {
    fn sep_width(&self) -> usize {
        if self.padded { 3 } else { 1 }
    }

    fn span_width(&self, col: usize, colspan: usize) -> usize {
        let end = (col + colspan).min(self.widths.len());
        let cols_sum: usize = self.widths[col..end].iter().sum();
        let seps = end.saturating_sub(col).saturating_sub(1) * self.sep_width();
        cols_sum + seps
    }

    fn total_width(&self) -> usize {
        let cols_sum: usize = self.widths.iter().sum();
        let seps = self.widths.len().saturating_sub(1) * self.sep_width();
        cols_sum + seps
    }
}

pub fn render_table(table: &TableData, width: usize) -> RenderedTable {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut styles = Vec::new();

    if let Some(caption) = table.caption.as_deref().map(str::trim)
        && !caption.is_empty()
    {
        for line in wrap_text(caption, width) {
            let line_idx = lines.len();
            let clipped = clip_text(&line, width);
            let len_chars = clipped.chars().count();
            lines.push(clipped);
            if len_chars > 0 {
                styles.push(TableStyleSpan {
                    line: line_idx,
                    col_chars: 0,
                    len_chars,
                    style: FontStyle::Bold,
                });
            }
        }
    }

    let grid = normalize_grid(&table.rows);
    if grid.slots.is_empty() || grid.ncols == 0 {
        return RenderedTable { lines, styles };
    }

    let body = match compute_column_layout(&grid, width) {
        Some(layout) => render_grid(&grid, &layout, width),
        None => render_stacked(&grid, width),
    };

    let base_line = lines.len();
    lines.extend(body.lines);
    styles.extend(body.styles.into_iter().map(|mut span| {
        span.line += base_line;
        span
    }));

    RenderedTable { lines, styles }
}

fn normalize_grid(rows: &[TableRow]) -> NormalizedGrid<'_> {
    let mut slots: Vec<Vec<GridSlot>> = Vec::with_capacity(rows.len());
    let mut remaining_rowspan: Vec<usize> = Vec::new();
    let mut ncols = 0usize;

    for (row_idx, row) in rows.iter().enumerate() {
        let mut row_slots = Vec::new();
        let mut col = 0usize;
        for (cell_idx, cell) in row.cells.iter().enumerate() {
            while col < remaining_rowspan.len() && remaining_rowspan[col] > 0 {
                row_slots.push(GridSlot::Covered);
                remaining_rowspan[col] -= 1;
                col += 1;
            }
            let colspan = cell.colspan.clamp(1, MAX_COLSPAN);
            let rowspan = cell.rowspan.clamp(1, MAX_ROWSPAN);
            if remaining_rowspan.len() < col + colspan {
                remaining_rowspan.resize(col + colspan, 0);
            }
            row_slots.push(GridSlot::Origin {
                row_idx,
                cell_idx,
                colspan,
            });
            for (offset, rem) in remaining_rowspan[col..col + colspan].iter_mut().enumerate() {
                if offset > 0 {
                    row_slots.push(GridSlot::Spanned);
                }
                *rem = rowspan.saturating_sub(1);
            }
            col += colspan;
        }
        while col < remaining_rowspan.len() {
            if remaining_rowspan[col] > 0 {
                row_slots.push(GridSlot::Covered);
                remaining_rowspan[col] -= 1;
            } else {
                row_slots.push(GridSlot::Empty);
            }
            col += 1;
        }
        ncols = ncols.max(row_slots.len());
        slots.push(row_slots);
    }

    for row_slots in &mut slots {
        if row_slots.len() < ncols {
            row_slots.resize(ncols, GridSlot::Empty);
        }
    }

    let (header_rows, rule_before) = detect_header_and_rules(rows, &slots);

    NormalizedGrid {
        rows,
        slots,
        ncols,
        header_rows,
        rule_before,
    }
}

fn detect_header_and_rules(rows: &[TableRow], slots: &[Vec<GridSlot>]) -> (usize, Vec<bool>) {
    let n = rows.len();
    let mut rule_before = vec![false; n];
    if n == 0 {
        return (0, rule_before);
    }

    let thead_count = rows.iter().take_while(|row| row.in_thead).count();
    let mut explicit_midrules = false;
    for r in 1..n {
        if rows[r].border_top || rows[r - 1].border_bottom {
            rule_before[r] = true;
            explicit_midrules = true;
        }
    }

    let header_rows = if thead_count > 0 && thead_count < n {
        rule_before[thead_count] = true;
        thead_count
    } else if explicit_midrules {
        (1..n).find(|&r| rule_before[r]).unwrap_or(1)
    } else {
        let leading_th_rows = rows
            .iter()
            .take_while(|row| !row.cells.is_empty() && row.cells.iter().all(|c| c.is_header))
            .count();
        if leading_th_rows > 0 && leading_th_rows < n {
            rule_before[leading_th_rows] = true;
            leading_th_rows
        } else if n > 1
            && slots[0]
                .iter()
                .any(|s| matches!(s, GridSlot::Origin { .. }))
        {
            rule_before[1] = true;
            1
        } else {
            0
        }
    };

    (header_rows, rule_before)
}

fn compute_column_layout(grid: &NormalizedGrid<'_>, width: usize) -> Option<ColumnLayout> {
    let ncols = grid.ncols;
    let mut natural = vec![1usize; ncols];
    let mut min_widths = vec![1usize; ncols];

    for row_slots in &grid.slots {
        for (col, slot) in row_slots.iter().enumerate() {
            if let GridSlot::Origin {
                row_idx,
                cell_idx,
                colspan: 1,
            } = *slot
            {
                let text = grid.rows[row_idx].cells[cell_idx].text.trim();
                let nat = natural_cell_width(text);
                natural[col] = natural[col].max(nat);
                if nat > 0 {
                    let word_min = min_cell_width(text);
                    min_widths[col] = min_widths[col].max(word_min.min(nat));
                }
            }
        }
    }

    // Expand columns when a colspan > 1 cell is wider than its spanned columns.
    for row_slots in &grid.slots {
        for (col, slot) in row_slots.iter().enumerate() {
            if let GridSlot::Origin {
                row_idx,
                cell_idx,
                colspan,
            } = *slot
                && colspan > 1
            {
                let end = (col + colspan).min(ncols);
                let span_cols = end - col;
                if span_cols == 0 {
                    continue;
                }
                let text = grid.rows[row_idx].cells[cell_idx].text.trim();
                let needed = natural_cell_width(text);
                let current: usize =
                    natural[col..end].iter().sum::<usize>() + 3 * span_cols.saturating_sub(1);
                if needed > current {
                    let extra = needed - current;
                    let per_col = extra / span_cols;
                    let rem = extra % span_cols;
                    for (i, w) in natural[col..end].iter_mut().enumerate() {
                        *w += per_col + usize::from(i < rem);
                    }
                }
            }
        }
    }

    let min_sum: usize = min_widths.iter().sum();
    let padded_seps = 3 * ncols.saturating_sub(1);
    let compact_seps = ncols.saturating_sub(1);

    let padded = if min_sum + padded_seps <= width {
        true
    } else if min_sum + compact_seps <= width
        || ncols * MIN_READABLE_COL_WIDTH + compact_seps <= width
    {
        false
    } else {
        return None;
    };

    let sep_total = if padded { padded_seps } else { compact_seps };
    let budget = width.saturating_sub(sep_total);
    let mut widths = natural;
    let total: usize = widths.iter().sum();
    if total > budget {
        shrink_columns(&mut widths, &min_widths, budget);
    }

    let used = widths.iter().sum::<usize>() + sep_total;
    let left_pad = width.saturating_sub(used) / 2;

    Some(ColumnLayout {
        widths,
        padded,
        left_pad,
    })
}

fn shrink_columns(widths: &mut [usize], min_widths: &[usize], budget: usize) {
    let floors: Vec<usize> = widths
        .iter()
        .zip(min_widths)
        .map(|(&w, &m)| m.min(w).max(1))
        .collect();
    let floor_sum: usize = floors.iter().sum();
    let effective_floors: Vec<usize> = if floor_sum <= budget {
        floors
    } else {
        vec![1usize; widths.len()]
    };

    let mut total: usize = widths.iter().sum();
    while total > budget {
        let Some((idx, _)) = widths
            .iter()
            .enumerate()
            .filter(|&(i, &w)| w > effective_floors[i])
            .max_by_key(|&(i, &w)| (w - effective_floors[i], w))
        else {
            break;
        };
        widths[idx] -= 1;
        total -= 1;
    }
}

fn render_grid(
    grid: &NormalizedGrid<'_>,
    layout: &ColumnLayout,
    max_width: usize,
) -> RenderedTable {
    let mut lines = Vec::new();
    let mut styles = Vec::new();
    let pad = " ".repeat(layout.left_pad);

    // Pre-wrap every row so we know whether body rows wrap to multiple lines.
    let wrapped_rows: Vec<Vec<Vec<String>>> = grid
        .slots
        .iter()
        .map(|row_slots| wrap_grid_row(grid, row_slots, layout))
        .collect();
    let multiline_body = wrapped_rows
        .iter()
        .skip(grid.header_rows)
        .any(|row_cells| row_cells.iter().any(|cell_lines| cell_lines.len() > 1));

    let top_rule = format!(
        "{pad}{}",
        render_rule_line(None, Some(&grid.slots[0]), layout, true)
    );
    lines.push(clip_text(&top_rule, max_width));

    for (r, row_slots) in grid.slots.iter().enumerate() {
        if r > 0 {
            let needs_rule = grid.rule_before[r] || (multiline_body && r > grid.header_rows);
            if needs_rule {
                let rule = format!(
                    "{pad}{}",
                    render_rule_line(Some(&grid.slots[r - 1]), Some(row_slots), layout, false)
                );
                lines.push(clip_text(&rule, max_width));
            }
        }

        let cell_lines = &wrapped_rows[r];
        let row_height = cell_lines.iter().map(Vec::len).max().unwrap_or(1).max(1);
        let sep = if layout.padded { " │ " } else { "│" };
        let is_header_row = r < grid.header_rows;

        for sub in 0..row_height {
            let line_idx = lines.len();
            let mut line = pad.clone();
            let mut col = 0usize;
            while col < grid.ncols {
                if col > 0 {
                    line.push_str(sep);
                }
                match row_slots[col] {
                    GridSlot::Origin {
                        row_idx,
                        cell_idx,
                        colspan,
                    } => {
                        let cell = &grid.rows[row_idx].cells[cell_idx];
                        let span_w = layout.span_width(col, colspan);
                        let raw = cell_lines[col].get(sub).map(String::as_str).unwrap_or("");
                        let aligned = align_cell_line(raw, span_w, cell.align);
                        let start_chars = line.chars().count();
                        let aligned_chars = aligned.chars().count();
                        line.push_str(&aligned);
                        if (is_header_row || cell.is_bold) && !raw.is_empty() {
                            styles.push(TableStyleSpan {
                                line: line_idx,
                                col_chars: start_chars,
                                len_chars: aligned_chars,
                                style: FontStyle::Bold,
                            });
                        }
                        col += colspan;
                    }
                    GridSlot::Covered | GridSlot::Empty => {
                        line.push_str(&" ".repeat(layout.widths[col]));
                        col += 1;
                    }
                    GridSlot::Spanned => {
                        col += 1;
                    }
                }
            }
            lines.push(clip_text(&line, max_width));
        }
    }

    let bottom_rule = format!(
        "{pad}{}",
        render_rule_line(grid.slots.last().map(Vec::as_slice), None, layout, true)
    );
    lines.push(clip_text(&bottom_rule, max_width));

    RenderedTable { lines, styles }
}

fn wrap_grid_row(
    grid: &NormalizedGrid<'_>,
    row_slots: &[GridSlot],
    layout: &ColumnLayout,
) -> Vec<Vec<String>> {
    let mut wrapped = vec![Vec::new(); grid.ncols];
    for (col, slot) in row_slots.iter().enumerate() {
        if let GridSlot::Origin {
            row_idx,
            cell_idx,
            colspan,
        } = *slot
        {
            let span_w = layout.span_width(col, colspan);
            let text = grid.rows[row_idx].cells[cell_idx].text.trim();
            wrapped[col] = wrap_cell_text(text, span_w);
        }
    }
    wrapped
}

fn render_rule_line(
    above: Option<&[GridSlot]>,
    below: Option<&[GridSlot]>,
    layout: &ColumnLayout,
    heavy: bool,
) -> String {
    let h = if heavy { '━' } else { '─' };
    let mut out = String::with_capacity(layout.total_width() * 3);
    for (col, &col_w) in layout.widths.iter().enumerate() {
        if col > 0 {
            let sep_above = above.is_some_and(|row| row[col] != GridSlot::Spanned);
            let sep_below = below.is_some_and(|row| row[col] != GridSlot::Spanned);
            let covered = below.is_some_and(|row| row[col] == GridSlot::Covered)
                && above.is_some_and(|row| row[col] != GridSlot::Empty);
            let left_covered = below.is_some_and(|row| row[col - 1] == GridSlot::Covered);
            let junction = if heavy {
                match (sep_above, sep_below) {
                    (false, true) => '┯',
                    (true, false) => '┷',
                    (true, true) => '┿',
                    (false, false) => '━',
                }
            } else if covered && left_covered {
                '│'
            } else if left_covered && !covered {
                '├'
            } else if !left_covered && covered {
                '┤'
            } else {
                match (sep_above, sep_below) {
                    (true, true) => '┼',
                    (true, false) => '┴',
                    (false, true) => '┬',
                    (false, false) => '─',
                }
            };
            if layout.padded {
                let left_h = if !heavy && left_covered { ' ' } else { h };
                let right_h = if !heavy && covered { ' ' } else { h };
                out.push(left_h);
                out.push(junction);
                out.push(right_h);
            } else {
                out.push(junction);
            }
        }
        let col_covered = !heavy && below.is_some_and(|row| row[col] == GridSlot::Covered);
        let fill = if col_covered { ' ' } else { h };
        for _ in 0..col_w {
            out.push(fill);
        }
    }
    out
}

fn render_stacked(grid: &NormalizedGrid<'_>, width: usize) -> RenderedTable {
    let mut lines = Vec::new();
    let mut styles = Vec::new();
    let headers = collect_stacked_headers(grid);
    let rule = "─".repeat(width);
    let heavy_rule = "━".repeat(width);

    lines.push(heavy_rule.clone());
    let start_row = if grid.header_rows < grid.slots.len() {
        grid.header_rows
    } else {
        0
    };
    let mut emitted_any = false;
    for r in start_row..grid.slots.len() {
        let row_slots = &grid.slots[r];
        let mut entries = Vec::new();
        for (col, slot) in row_slots.iter().enumerate() {
            if let GridSlot::Origin {
                row_idx, cell_idx, ..
            } = *slot
            {
                let text = grid.rows[row_idx].cells[cell_idx].text.trim();
                if !text.is_empty() {
                    entries.push((col, text, grid.rows[row_idx].cells[cell_idx].is_bold));
                }
            }
        }
        if entries.is_empty() {
            continue;
        }
        if emitted_any {
            lines.push(rule.clone());
        }
        emitted_any = true;
        for (idx, (col, value, is_bold)) in entries.into_iter().enumerate() {
            let label = headers.get(col).map(String::as_str).unwrap_or("");
            let formatted = if idx == 0 && (label.is_empty() || col == 0) {
                value.to_owned()
            } else if !label.is_empty() {
                format!("{label}: {value}")
            } else {
                value.to_owned()
            };
            for wrapped in wrap_text(&formatted, width) {
                let line_idx = lines.len();
                let clipped = clip_text(&wrapped, width);
                if idx == 0 || is_bold {
                    styles.push(TableStyleSpan {
                        line: line_idx,
                        col_chars: 0,
                        len_chars: clipped.chars().count(),
                        style: FontStyle::Bold,
                    });
                }
                lines.push(clipped);
            }
        }
    }
    lines.push(heavy_rule);

    RenderedTable { lines, styles }
}

fn collect_stacked_headers(grid: &NormalizedGrid<'_>) -> Vec<String> {
    let mut headers = vec![String::new(); grid.ncols];
    for r in 0..grid.header_rows {
        for (col, slot) in grid.slots[r].iter().enumerate() {
            if let GridSlot::Origin {
                row_idx,
                cell_idx,
                colspan,
            } = *slot
            {
                let text = grid.rows[row_idx].cells[cell_idx].text.trim();
                if text.is_empty() {
                    continue;
                }
                for header in headers.iter_mut().skip(col).take(colspan) {
                    if !header.is_empty() {
                        header.push(' ');
                    }
                    header.push_str(text);
                }
            }
        }
    }
    headers
}

fn natural_cell_width(text: &str) -> usize {
    text.lines()
        .map(|line| cell_width(line.trim()))
        .max()
        .unwrap_or(0)
}

fn min_cell_width(text: &str) -> usize {
    let longest_word = text.split_whitespace().map(cell_width).max().unwrap_or(0);
    longest_word.clamp(1, 12)
}

fn wrap_cell_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    if text.is_empty() {
        return vec![String::new()];
    }
    let mut out = Vec::new();
    for segment in text.split('\n') {
        let trimmed = segment.trim();
        if trimmed.is_empty() {
            continue;
        }
        for line in wrap_text(trimmed, width) {
            out.push(clip_text(&line, width));
        }
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

fn align_cell_line(text: &str, width: usize, align: CellAlign) -> String {
    let clipped = clip_text(text, width);
    let text_w = cell_width(&clipped);
    let pad = width.saturating_sub(text_w);
    match align {
        CellAlign::Left => format!("{clipped}{}", " ".repeat(pad)),
        CellAlign::Right => format!("{}{clipped}", " ".repeat(pad)),
        CellAlign::Center => {
            let left = pad / 2;
            let right = pad - left;
            format!("{}{clipped}{}", " ".repeat(left), " ".repeat(right))
        }
    }
}
