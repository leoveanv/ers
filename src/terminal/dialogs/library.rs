use super::render::safe_text;
use crate::app::modes::library::{CATEGORY_CHOICES, LibraryMode, LibraryPhase};
use crate::app::{ReaderMode, ReaderModel};
use crate::book::{LibraryCategory, LibraryEntry, format_library_tags, parse_library_tags};
use crate::document::text::{cell_width, clip_text};
use crate::terminal::widgets::list::{ListRows, dialog_window, draw_rows};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

const SEPARATOR: &str = " | ";
const SEPARATOR_WIDTH: usize = 3;
const PROG_COL_WIDTH: usize = 4;
const CATEGORY_COL_WIDTH: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ColumnWidths {
    prog: usize,
    title: usize,
    author: usize,
    category: usize,
    tags: usize,
}

fn format_progress(progress: Option<f64>) -> String {
    progress.map_or_else(|| "…".into(), |value| format!("{:.0}%", value * 100.0))
}

fn entry_title(entry: &LibraryEntry) -> &str {
    entry.title.as_deref().unwrap_or("Untitled")
}

fn entry_author(entry: &LibraryEntry) -> &str {
    entry.author.as_deref().unwrap_or("Unknown author")
}

fn entry_category(entry: &LibraryEntry) -> &str {
    entry.category.map(LibraryCategory::as_str).unwrap_or("—")
}

fn entry_tags(entry: &LibraryEntry) -> String {
    format_library_tags(&entry.tags)
}

fn fit_cell(value: &str, width: usize, right_align: bool) -> String {
    if width == 0 {
        return String::new();
    }
    let clean = safe_text(value);
    let current = cell_width(&clean);
    let fitted = if current > width {
        if width == 1 {
            clip_text(&clean, 1)
        } else {
            let mut clipped = clip_text(&clean, width - 1);
            clipped.push('…');
            clipped
        }
    } else {
        clean
    };
    let fitted_width = cell_width(&fitted);
    let padding = " ".repeat(width.saturating_sub(fitted_width));
    if right_align {
        format!("{padding}{fitted}")
    } else {
        format!("{fitted}{padding}")
    }
}

fn compute_column_widths(entries: &[&LibraryEntry], total_width: usize) -> ColumnWidths {
    let separators = SEPARATOR_WIDTH * 4;
    let fixed = PROG_COL_WIDTH + CATEGORY_COL_WIDTH + separators;
    if total_width <= fixed + 3 {
        let flex = total_width.saturating_sub(separators);
        let prog = PROG_COL_WIDTH.min(flex);
        let rest = flex.saturating_sub(prog);
        let title = rest / 2;
        let author = (rest - title) / 2;
        let category = (rest - title - author) / 2;
        let tags = rest - title - author - category;
        return ColumnWidths {
            prog,
            title,
            author,
            category,
            tags,
        };
    }

    let natural_title = entries
        .iter()
        .map(|entry| cell_width(&safe_text(entry_title(entry))))
        .max()
        .unwrap_or(0)
        .max(5);
    let natural_author = entries
        .iter()
        .map(|entry| cell_width(&safe_text(entry_author(entry))))
        .max()
        .unwrap_or(0)
        .max(6);
    let natural_tags = entries
        .iter()
        .map(|entry| cell_width(&safe_text(&entry_tags(entry))))
        .max()
        .unwrap_or(0)
        .max(4);

    let budget = total_width - fixed;
    let [title, author, tags] = distribute_widths(
        [natural_title, natural_author, natural_tags],
        [50, 25, 25],
        budget,
    );

    ColumnWidths {
        prog: PROG_COL_WIDTH,
        title,
        author,
        category: CATEGORY_COL_WIDTH,
        tags,
    }
}

fn distribute_widths(natural: [usize; 3], weights: [usize; 3], budget: usize) -> [usize; 3] {
    let total_natural: usize = natural.iter().sum();
    if total_natural <= budget {
        return natural;
    }

    let weight_sum: usize = weights.iter().sum();
    let mut allocated = [0usize; 3];
    let mut remaining = budget;

    // First pass: assign proportional shares capped at each column's natural width.
    for index in 0..3 {
        let share = (budget * weights[index]) / weight_sum;
        let width = share.min(natural[index]).min(remaining);
        allocated[index] = width;
        remaining = remaining.saturating_sub(width);
    }

    // Second pass: redistribute leftover budget to columns that still need more space.
    while remaining > 0 {
        let mut progressed = false;
        for index in 0..3 {
            if remaining == 0 {
                break;
            }
            if allocated[index] < natural[index] {
                allocated[index] += 1;
                remaining -= 1;
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }

    allocated
}

fn format_header_row(widths: ColumnWidths) -> String {
    format!(
        "{}{}{}{}{}{}{}{}{}",
        fit_cell("Prog", widths.prog, true),
        SEPARATOR,
        fit_cell("Title", widths.title, false),
        SEPARATOR,
        fit_cell("Author", widths.author, false),
        SEPARATOR,
        fit_cell("Category", widths.category, false),
        SEPARATOR,
        fit_cell("Tags", widths.tags, false),
    )
}

fn format_entry_row(entry: &LibraryEntry, widths: ColumnWidths) -> String {
    format!(
        "{}{}{}{}{}{}{}{}{}",
        fit_cell(&format_progress(entry.reading_progress), widths.prog, true),
        SEPARATOR,
        fit_cell(entry_title(entry), widths.title, false),
        SEPARATOR,
        fit_cell(entry_author(entry), widths.author, false),
        SEPARATOR,
        fit_cell(entry_category(entry), widths.category, false),
        SEPARATOR,
        fit_cell(&entry_tags(entry), widths.tags, false),
    )
}

fn entry_summary(entry: &LibraryEntry) -> String {
    format!(
        "{} ({})",
        safe_text(entry_title(entry)),
        safe_text(entry_author(entry)),
    )
}

pub(in crate::terminal) fn draw(frame: &mut Frame<'_>, model: &ReaderModel, style: Style) {
    let ReaderMode::Library(mode) = model.view().mode else {
        return;
    };
    let inside = dialog_window(frame, "Library", style);

    match &mode.phase {
        LibraryPhase::Opening(path) => {
            draw_message(
                frame,
                inside,
                &format!(
                    "Opening {}…\n\nEscape cancels and keeps the current book open.",
                    path
                ),
                style,
            );
            return;
        }
        LibraryPhase::Waiting { .. } => {
            draw_message(
                frame,
                inside,
                "Saving position and updating library…\n\nEscape cancels the dialog.",
                style,
            );
            return;
        }
        LibraryPhase::ConfirmRemove { .. } => {
            let summary = mode
                .selected_entry()
                .map_or_else(String::new, entry_summary);
            draw_message(
                frame,
                inside,
                &format!(
                    "Remove {summary} from the library?\n\nThe saved position and bookmarks will be removed. The ebook, annotations, and Markdown exports remain.\n\ny: remove   n/Escape: cancel"
                ),
                style,
            );
            return;
        }
        _ => {}
    }

    let show_card = matches!(
        mode.phase,
        LibraryPhase::Card
            | LibraryPhase::EditingTags {
                return_to_card: true,
                ..
            }
            | LibraryPhase::ChoosingCategory {
                return_to_card: true,
                ..
            }
    );

    if show_card {
        draw_card_view(frame, inside, mode, style);
    } else {
        draw_table_view(frame, inside, mode, model.view().list_top, style);
    }

    match &mode.phase {
        LibraryPhase::EditingTags { input, .. } => {
            draw_tags_popup(frame, inside, mode.selected_entry(), input, style);
        }
        LibraryPhase::ChoosingCategory { selected, .. } => {
            draw_category_popup(frame, inside, mode.selected_entry(), *selected, style);
        }
        _ => {}
    }
}

fn draw_message(frame: &mut Frame<'_>, area: Rect, message: &str, style: Style) {
    frame.render_widget(
        Paragraph::new(safe_text_lines(message))
            .style(style)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_table_view(
    frame: &mut Frame<'_>,
    inside: Rect,
    mode: &LibraryMode,
    list_top: usize,
    style: Style,
) {
    if inside.height == 0 {
        return;
    }

    let visible = mode.visible_entries();
    let searching = matches!(mode.phase, LibraryPhase::Searching);

    // Row 0: Search bar
    draw_search_bar(
        frame,
        Rect::new(inside.x, inside.y, inside.width, 1),
        &mode.query,
        searching,
        visible.len(),
        mode.entries.len(),
        style,
    );

    // Row 1: Aligned column header
    let widths = compute_column_widths(&visible, inside.width as usize);
    if inside.height > 1 {
        let header = clip_text(&format_header_row(widths), inside.width as usize);
        frame.render_widget(
            Paragraph::new(header).style(style.add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
            Rect::new(inside.x, inside.y + 1, inside.width, 1),
        );
    }

    // Rows 2 .. height-1: Library entries (or empty state)
    let list_height = inside.height.saturating_sub(3);
    if list_height > 0 {
        let list_area = Rect::new(inside.x, inside.y + 2, inside.width, list_height);
        if mode.entries.is_empty() {
            draw_message(
                frame,
                list_area,
                "No reading history. Escape closes.",
                style,
            );
        } else if visible.is_empty() {
            draw_message(
                frame,
                list_area,
                "No library entries match your search. Press Escape to clear the filter.",
                style,
            );
        } else {
            draw_rows(
                frame,
                ListRows {
                    area: list_area,
                    selected: mode.selected,
                    first: list_top,
                    style,
                },
                visible.iter().map(|entry| format_entry_row(entry, widths)),
            );
        }
    }

    // Bottom row: Footer hints
    if inside.height > 2 {
        let footer = if searching {
            "Type to filter (title, author, category, #tag)   Enter: done   Ctrl-U: clear   Esc: cancel"
        } else {
            "Enter: open  /: search  s: card  t: tags  c: category  d: remove  Esc/R: close"
        };
        frame.render_widget(
            Paragraph::new(clip_text(footer, inside.width as usize)).style(style),
            Rect::new(inside.x, inside.y + inside.height - 1, inside.width, 1),
        );
    }
}

fn draw_search_bar(
    frame: &mut Frame<'_>,
    area: Rect,
    query: &str,
    searching: bool,
    visible_count: usize,
    total_count: usize,
    style: Style,
) {
    let width = area.width as usize;
    let count_text = if query.is_empty() {
        format!("{total_count} items")
    } else {
        format!("{visible_count}/{total_count} matches")
    };
    let left_text = if searching {
        format!("Search: {}_", safe_text(query))
    } else if query.is_empty() {
        "Search: (/ to filter)".into()
    } else {
        format!("Search: {}  (Esc clears)", safe_text(query))
    };

    let count_width = cell_width(&count_text);
    let bar_text = if width > count_width + 2 {
        let left_budget = width - count_width - 1;
        let left = fit_cell(&left_text, left_budget, false);
        format!("{left} {count_text}")
    } else {
        clip_text(&left_text, width)
    };

    let bar_style = if searching {
        style.add_modifier(Modifier::BOLD | Modifier::REVERSED)
    } else {
        style.add_modifier(Modifier::BOLD)
    };
    frame.render_widget(Paragraph::new(bar_text).style(bar_style), area);
}

fn draw_card_view(frame: &mut Frame<'_>, inside: Rect, mode: &LibraryMode, style: Style) {
    let visible = mode.visible_entries();
    let Some(entry) = mode.selected_entry() else {
        draw_message(frame, inside, "No library entry selected.", style);
        return;
    };

    let body_height = inside.height.saturating_sub(1);
    let card_width = inside.width.min(96);
    let card_height = body_height.min(18).max(body_height.min(12));
    let card_x = inside.x + inside.width.saturating_sub(card_width) / 2;
    let card_y = inside.y + body_height.saturating_sub(card_height) / 2;
    let card_area = Rect::new(card_x, card_y, card_width, card_height);

    let card_title = format!(
        " Entry Card ({}/{}) ",
        mode.selected + 1,
        visible.len().max(1)
    );
    frame.render_widget(Clear, card_area);
    let block = Block::default()
        .title(card_title)
        .borders(Borders::ALL)
        .style(style);
    let card_inner = block.inner(card_area);
    frame.render_widget(block, card_area);

    let progress = entry.reading_progress.map_or_else(
        || "Not started".into(),
        |value| format!("{:.1}%", value * 100.0),
    );
    let category = entry
        .category
        .map(LibraryCategory::as_str)
        .unwrap_or("(none)");
    let tags = if entry.tags.is_empty() {
        "(none)".into()
    } else {
        format_library_tags(&entry.tags)
    };

    let label_style = style.add_modifier(Modifier::BOLD);
    let lines = vec![
        card_field("Title:     ", entry_title(entry), label_style, style),
        card_field("Author:    ", entry_author(entry), label_style, style),
        card_field("Category:  ", category, label_style, style),
        card_field("Tags:      ", &tags, label_style, style),
        card_field("Progress:  ", &progress, label_style, style),
        card_field("Last read: ", &entry.last_read, label_style, style),
        Line::from(""),
        card_field("Path:      ", &entry.filepath, label_style, style),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .style(style)
            .wrap(Wrap { trim: false }),
        card_inner,
    );

    if inside.height > 0 {
        let footer = "Enter: open  t: tags  c: category  j/k: prev/next  d: remove  s/Esc: back";
        frame.render_widget(
            Paragraph::new(clip_text(footer, inside.width as usize)).style(style),
            Rect::new(inside.x, inside.y + inside.height - 1, inside.width, 1),
        );
    }
}

fn card_field(
    label: &'static str,
    value: &str,
    label_style: Style,
    value_style: Style,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(label, label_style),
        Span::styled(safe_text(value), value_style),
    ])
}

fn draw_tags_popup(
    frame: &mut Frame<'_>,
    inside: Rect,
    entry: Option<&LibraryEntry>,
    input: &str,
    style: Style,
) {
    let area = centered_popup(inside, 76, 8);
    frame.render_widget(Clear, area);
    let block = Block::default()
        .title(" Edit Tags ")
        .borders(Borders::ALL)
        .style(style);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let title = entry.map_or("Untitled", entry_title);
    let parsed = parse_library_tags(input);
    let preview = if parsed.is_empty() {
        "(none)".into()
    } else {
        format_library_tags(&parsed)
    };
    let label_style = style.add_modifier(Modifier::BOLD);
    let lines = vec![
        card_field("Entry:   ", title, label_style, style),
        Line::from(vec![
            Span::styled("Tags:    ", label_style),
            Span::styled(
                format!("{}_", safe_text(input)),
                style.add_modifier(Modifier::REVERSED),
            ),
        ]),
        card_field("Preview: ", &preview, label_style, style),
        Line::from(""),
        Line::from(Span::styled(
            "Space or comma separated (# optional)  Enter: save  Ctrl-U: clear  Esc: cancel",
            style,
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .style(style)
            .wrap(Wrap { trim: false }),
        inner,
    );
}

fn draw_category_popup(
    frame: &mut Frame<'_>,
    inside: Rect,
    entry: Option<&LibraryEntry>,
    selected: usize,
    style: Style,
) {
    let area = centered_popup(inside, 54, 10);
    frame.render_widget(Clear, area);
    let block = Block::default()
        .title(" Select Category ")
        .borders(Borders::ALL)
        .style(style);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let title = entry.map_or("Untitled", entry_title);
    let mut lines = vec![
        card_field("Entry: ", title, style.add_modifier(Modifier::BOLD), style),
        Line::from(""),
    ];
    for (index, choice) in CATEGORY_CHOICES.iter().enumerate() {
        let label = match choice {
            Some(LibraryCategory::Book) => "1. book",
            Some(LibraryCategory::Webpage) => "2. webpage",
            Some(LibraryCategory::Paper) => "3. paper",
            None => "0. (none)",
        };
        let row_style = if index == selected {
            style.add_modifier(Modifier::REVERSED)
        } else {
            style
        };
        lines.push(Line::from(Span::styled(format!(" {label} "), row_style)));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Enter/1-3/b/w/p: select  0/x: clear  Esc: cancel",
        style,
    )));

    frame.render_widget(
        Paragraph::new(lines)
            .style(style)
            .wrap(Wrap { trim: false }),
        inner,
    );
}

fn centered_popup(container: Rect, max_width: u16, max_height: u16) -> Rect {
    let width = container.width.min(max_width).max(1);
    let height = container.height.min(max_height).max(1);
    let x = container.x + container.width.saturating_sub(width) / 2;
    let y = container.y + container.height.saturating_sub(height) / 2;
    Rect::new(x, y, width, height)
}

fn safe_text_lines(value: &str) -> String {
    value
        .split('\n')
        .map(safe_text)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry(
        title: &str,
        author: &str,
        path: &str,
        category: Option<LibraryCategory>,
        tags: &[&str],
    ) -> LibraryEntry {
        LibraryEntry {
            last_read: "2026-11-22 19:00:00".into(),
            filepath: path.into(),
            title: Some(title.into()),
            author: Some(author.into()),
            reading_progress: Some(0.42),
            category,
            tags: tags.iter().map(|tag| (*tag).into()).collect(),
        }
    }

    #[test]
    fn library_rows_align_pipes_and_omit_filepaths() {
        let first = sample_entry(
            "Short",
            "Ada",
            "/very/long/secret/path/that/should/not/appear.epub",
            Some(LibraryCategory::Book),
            &["scifi"],
        );
        let second = sample_entry(
            "A Much Longer Title For Alignment Check",
            "Grace Hopper",
            "https://example.com/extremely/long/url/with/query?a=1&b=2",
            Some(LibraryCategory::Webpage),
            &["ai", "notes"],
        );
        let entries = [&first, &second];
        let widths = compute_column_widths(&entries, 100);
        let header = format_header_row(widths);
        let row1 = format_entry_row(&first, widths);
        let row2 = format_entry_row(&second, widths);

        let pipe_positions = |line: &str| -> Vec<usize> {
            let mut col = 0;
            let mut positions = Vec::new();
            for ch in line.chars() {
                if ch == '|' {
                    positions.push(col);
                }
                col += cell_width(&ch.to_string());
            }
            positions
        };

        assert_eq!(pipe_positions(&header), pipe_positions(&row1));
        assert_eq!(pipe_positions(&row1), pipe_positions(&row2));
        assert_eq!(pipe_positions(&header).len(), 4);
        assert!(!row1.contains("/very/long/secret"));
        assert!(!row2.contains("https://example.com"));
        assert!(row1.contains("book"));
        assert!(row1.contains("#scifi"));
        assert!(row2.contains("webpage"));
        assert!(row2.contains("#ai #notes"));
    }

    #[test]
    fn narrow_terminal_keeps_aligned_pipes_within_width() {
        let first = sample_entry(
            "The Three-Body Problem",
            "Cixin Liu",
            "/path/to/three-body.epub",
            Some(LibraryCategory::Paper),
            &["physics", "classic"],
        );
        let entries = [&first];
        for width in [30, 48, 80, 120] {
            let widths = compute_column_widths(&entries, width);
            let header = format_header_row(widths);
            let row = format_entry_row(&first, widths);
            assert!(
                cell_width(&header) <= width,
                "header width {} > {width}: {header:?}",
                cell_width(&header)
            );
            assert!(
                cell_width(&row) <= width,
                "row width {} > {width}: {row:?}",
                cell_width(&row)
            );
        }
    }

    #[test]
    fn card_renders_full_path_and_metadata_while_table_hides_path() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let path = "/Users/reader/Zotero/storage/ABCDEF12/Very_Long_Paper_Filename_2026.epub";
        let entry = sample_entry(
            "Scaling Discovery",
            "Jongho Park",
            path,
            Some(LibraryCategory::Paper),
            &["agents", "ml"],
        );
        let mut mode = LibraryMode::from_entries(vec![entry]);

        let render_to_string = |mode: &LibraryMode| -> String {
            let backend = TestBackend::new(100, 20);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal
                .draw(|frame| {
                    let inside = Rect::new(0, 0, 100, 20);
                    if matches!(mode.phase, LibraryPhase::Card) {
                        draw_card_view(frame, inside, mode, Style::default());
                    } else {
                        draw_table_view(frame, inside, mode, 0, Style::default());
                    }
                })
                .unwrap();
            let buffer = terminal.backend().buffer();
            (0..buffer.area.height)
                .map(|y| {
                    (0..buffer.area.width)
                        .map(|x| buffer[(x, y)].symbol())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n")
        };

        let table_output = render_to_string(&mode);
        assert!(table_output.contains("Search:"), "{table_output}");
        assert!(table_output.contains("Scaling Discovery"), "{table_output}");
        assert!(table_output.contains("paper"), "{table_output}");
        assert!(table_output.contains("#agents #ml"), "{table_output}");
        assert!(!table_output.contains("Zotero/storage"), "{table_output}");

        mode.phase = LibraryPhase::Card;
        let card_output = render_to_string(&mode);
        assert!(card_output.contains("Entry Card"), "{card_output}");
        assert!(card_output.contains("Scaling Discovery"), "{card_output}");
        assert!(card_output.contains("paper"), "{card_output}");
        assert!(card_output.contains("#agents #ml"), "{card_output}");
        assert!(card_output.contains(path), "{card_output}");
    }
}
