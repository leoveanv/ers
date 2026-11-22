use std::collections::VecDeque;
use std::sync::Arc;

use super::{ReaderEffect, ReaderMode, ReaderModel};
use crate::app::requests::{LibraryCommand, LibraryJob, LibraryReply, LibraryResult};
use crate::book::{LibraryCategory, LibraryEntry, format_library_tags, parse_library_tags};
use crate::configuration::{Command, InputKey};
use crate::document::text::graphemes;

pub const CATEGORY_CHOICES: [Option<LibraryCategory>; 4] = [
    Some(LibraryCategory::Book),
    Some(LibraryCategory::Webpage),
    Some(LibraryCategory::Paper),
    None,
];

#[derive(Default)]
pub(in crate::app) struct LibraryState {
    requests: super::RequestSequence,
}

impl LibraryState {
    fn start(&mut self, command: LibraryCommand) -> LibraryJob {
        LibraryJob {
            request_id: self.requests.next(),
            command,
        }
    }
}

#[derive(Clone, Debug)]
pub enum LibraryPhase {
    Browsing,
    Searching,
    Card,
    EditingTags {
        input: String,
        return_to_card: bool,
    },
    ChoosingCategory {
        selected: usize,
        return_to_card: bool,
    },
    ConfirmRemove {
        return_to_card: bool,
    },
    Waiting {
        request_id: u64,
        command: LibraryCommand,
        return_to_card: bool,
    },
    Opening(String),
}

#[derive(Clone, Debug)]
pub struct LibraryMode {
    pub entries: Vec<LibraryEntry>,
    pub query: String,
    pub selected: usize,
    pub phase: LibraryPhase,
    prefix: String,
    queued: VecDeque<LibraryInput>,
    ignored_input: bool,
}

#[derive(Clone, Debug)]
enum LibraryInput {
    Key(InputKey),
    Motion { key: InputKey, repetitions: usize },
}

impl LibraryInput {
    fn from_key(key: InputKey, command: Option<Command>) -> Self {
        if matches!(
            command,
            Some(Command::ScrollDown | Command::ScrollUp | Command::PageDown | Command::PageUp)
        ) {
            Self::Motion {
                key,
                repetitions: 1,
            }
        } else {
            Self::Key(key)
        }
    }
}

impl LibraryMode {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            query: String::new(),
            selected: 0,
            phase: LibraryPhase::Browsing,
            prefix: String::new(),
            queued: VecDeque::new(),
            ignored_input: false,
        }
    }

    #[cfg(test)]
    pub(crate) fn from_entries(entries: Vec<LibraryEntry>) -> Self {
        Self {
            entries,
            ..Self::new()
        }
    }

    pub fn visible_entries(&self) -> Vec<&LibraryEntry> {
        if self.query.trim().is_empty() {
            return self.entries.iter().collect();
        }
        self.entries
            .iter()
            .filter(|entry| entry_matches_query(entry, &self.query))
            .collect()
    }

    pub fn selected_entry(&self) -> Option<&LibraryEntry> {
        self.visible_entries().get(self.selected).copied()
    }

    fn selected_path(&self) -> Option<String> {
        self.selected_entry().map(|entry| entry.filepath.clone())
    }

    fn select_by_path(&mut self, path: &str) {
        let visible = self.visible_entries();
        if let Some(index) = visible.iter().position(|entry| entry.filepath == path) {
            self.selected = index;
        } else {
            self.selected = self.selected.min(visible.len().saturating_sub(1));
        }
    }

    fn clamp_selection(&mut self) {
        let count = self.visible_entries().len();
        self.selected = self.selected.min(count.saturating_sub(1));
    }

    fn queue(&mut self, input: LibraryInput) -> bool {
        if let (
            Some(LibraryInput::Motion {
                key: previous,
                repetitions,
            }),
            LibraryInput::Motion {
                key,
                repetitions: incoming,
            },
        ) = (self.queued.back_mut(), &input)
            && previous == key
        {
            *repetitions = repetitions.saturating_add(*incoming);
            return true;
        }
        if self.queued.len() < 128 {
            self.queued.push_back(input);
            true
        } else {
            self.ignored_input = true;
            false
        }
    }
}

pub fn entry_matches_query(entry: &LibraryEntry, query: &str) -> bool {
    let title = entry.title.as_deref().unwrap_or("Untitled").to_lowercase();
    let author = entry
        .author
        .as_deref()
        .unwrap_or("Unknown author")
        .to_lowercase();
    let category = entry
        .category
        .map(LibraryCategory::as_str)
        .unwrap_or("")
        .to_lowercase();
    let tags = format_library_tags(&entry.tags).to_lowercase();
    let path = entry.filepath.to_lowercase();

    query.split_whitespace().all(|raw_term| {
        let term = raw_term.to_lowercase();
        if let Some(tag_term) = term.strip_prefix('#') {
            if tag_term.is_empty() {
                !entry.tags.is_empty()
            } else {
                entry
                    .tags
                    .iter()
                    .any(|tag| tag.to_lowercase().contains(tag_term))
            }
        } else {
            title.contains(&term)
                || author.contains(&term)
                || category.contains(&term)
                || tags.contains(&term)
                || path.contains(&term)
        }
    })
}

fn pop_grapheme(value: &mut String) {
    let count = graphemes(value).last().map_or(0, |cluster| cluster.len());
    value.truncate(value.len().saturating_sub(count));
}

fn sanitize_single_line(text: &str) -> String {
    text.chars()
        .map(|ch| if ch == '\n' || ch == '\r' { ' ' } else { ch })
        .filter(|ch| !ch.is_control())
        .collect()
}

fn category_choice_index(category: Option<LibraryCategory>) -> usize {
    CATEGORY_CHOICES
        .iter()
        .position(|choice| *choice == category)
        .unwrap_or(0)
}

impl ReaderModel {
    pub fn can_autosave(&self) -> bool {
        !self.loading && !self.quitting && !matches!(self.mode, ReaderMode::Library(_))
    }

    pub(in crate::app) fn begin_library(&mut self) -> Vec<ReaderEffect> {
        self.status = "Saving position and loading library…".into();
        self.submit_library(
            LibraryMode::new(),
            LibraryCommand::List(self.save_request(false)),
            false,
        )
    }

    fn submit_library(
        &mut self,
        mut mode: LibraryMode,
        command: LibraryCommand,
        return_to_card: bool,
    ) -> Vec<ReaderEffect> {
        let job = self.library_state.start(command);
        mode.phase = LibraryPhase::Waiting {
            request_id: job.request_id,
            command: job.command.clone(),
            return_to_card,
        };
        self.mode = ReaderMode::Library(Box::new(mode));
        vec![ReaderEffect::Library(job)]
    }

    pub(in crate::app) fn handle_library_paste(&mut self, text: &str) {
        let ReaderMode::Library(mode) = &mut self.mode else {
            return;
        };
        let sanitized = sanitize_single_line(text);
        if sanitized.is_empty() {
            return;
        }
        match &mut mode.phase {
            LibraryPhase::Searching => {
                let current_path = mode.selected_path();
                mode.query.push_str(&sanitized);
                if let Some(path) = current_path {
                    mode.select_by_path(&path);
                } else {
                    mode.clamp_selection();
                }
            }
            LibraryPhase::EditingTags { input, .. } => {
                input.push_str(&sanitized);
            }
            _ => {}
        }
    }

    pub(in crate::app) fn handle_library_key(&mut self, key: InputKey) -> Vec<ReaderEffect> {
        let ReaderMode::Library(mut mode) = std::mem::replace(&mut self.mode, ReaderMode::Reading)
        else {
            return Vec::new();
        };
        let command = self.configuration.bindings.command_for(key);
        let opening = matches!(mode.phase, LibraryPhase::Opening(_));
        if key == InputKey::Character('\u{03}') {
            let mut effects = if opening {
                vec![ReaderEffect::CancelBookOpen]
            } else {
                Vec::new()
            };
            effects.extend(self.execute(Command::Quit, 1));
            return effects;
        }

        // #NOTE: Text-entry phases consume printable characters like 'q' and 'R' before global close shortcuts.
        if matches!(mode.phase, LibraryPhase::Searching) {
            return self.library_search_key(*mode, key);
        }
        if matches!(mode.phase, LibraryPhase::EditingTags { .. }) {
            return self.library_tags_key(*mode, key);
        }
        if matches!(mode.phase, LibraryPhase::ChoosingCategory { .. }) {
            return self.library_category_key(*mode, key, command);
        }

        if key == InputKey::Escape && !mode.prefix.is_empty() {
            mode.prefix.clear();
        } else if matches!(mode.phase, LibraryPhase::Card)
            && matches!(key, InputKey::Escape | InputKey::Character('s'))
        {
            mode.phase = LibraryPhase::Browsing;
        } else if matches!(mode.phase, LibraryPhase::Browsing)
            && key == InputKey::Escape
            && !mode.query.is_empty()
        {
            let current_path = mode.selected_path();
            mode.query.clear();
            if let Some(path) = current_path {
                mode.select_by_path(&path);
            } else {
                mode.clamp_selection();
            }
        } else if matches!(key, InputKey::Escape | InputKey::Character('q'))
            || matches!(command, Some(Command::Library | Command::Quit))
        {
            self.status.clear();
            if opening {
                mode.phase = LibraryPhase::Browsing;
                mode.queued.clear();
                self.mode = ReaderMode::Library(mode);
                let mut effects = vec![ReaderEffect::CancelBookOpen];
                effects.extend(self.resize(self.viewport));
                return effects;
            }
            if let LibraryPhase::ConfirmRemove { return_to_card } = mode.phase {
                mode.phase = if return_to_card {
                    LibraryPhase::Card
                } else {
                    LibraryPhase::Browsing
                };
            } else {
                return Vec::new();
            }
        } else {
            match mode.phase {
                LibraryPhase::Waiting { .. } => {
                    if !mode.queue(LibraryInput::from_key(key, command)) {
                        self.status = "Input queue full. New input was ignored.".into();
                    }
                }
                LibraryPhase::Opening(_) => {}
                LibraryPhase::ConfirmRemove { return_to_card } => {
                    if matches!(key, InputKey::Character('y' | 'Y')) {
                        return self.remove_library_entry(*mode);
                    } else {
                        mode.phase = if return_to_card {
                            LibraryPhase::Card
                        } else {
                            LibraryPhase::Browsing
                        };
                    }
                }
                LibraryPhase::Card => return self.library_card_key(*mode, key, command),
                LibraryPhase::Browsing => return self.library_browse_key(*mode, key, command),
                LibraryPhase::Searching
                | LibraryPhase::EditingTags { .. }
                | LibraryPhase::ChoosingCategory { .. } => {}
            }
        }
        self.mode = ReaderMode::Library(mode);
        Vec::new()
    }

    fn library_search_key(&mut self, mut mode: LibraryMode, key: InputKey) -> Vec<ReaderEffect> {
        let current_path = mode.selected_path();
        match key {
            InputKey::Escape => {
                mode.query.clear();
                mode.phase = LibraryPhase::Browsing;
                if let Some(path) = current_path {
                    mode.select_by_path(&path);
                } else {
                    mode.clamp_selection();
                }
            }
            InputKey::Enter => {
                mode.phase = LibraryPhase::Browsing;
            }
            InputKey::Down => {
                let count = mode.visible_entries().len();
                mode.selected = mode.selected.saturating_add(1).min(count.saturating_sub(1));
            }
            InputKey::Up => {
                mode.selected = mode.selected.saturating_sub(1);
            }
            InputKey::Backspace | InputKey::Character('\u{7f}' | '\u{08}') => {
                pop_grapheme(&mut mode.query);
                if let Some(path) = current_path {
                    mode.select_by_path(&path);
                } else {
                    mode.clamp_selection();
                }
            }
            InputKey::Character('\u{15}') => {
                mode.query.clear();
                if let Some(path) = current_path {
                    mode.select_by_path(&path);
                } else {
                    mode.clamp_selection();
                }
            }
            InputKey::Character(ch) if !ch.is_control() => {
                mode.query.push(ch);
                if let Some(path) = current_path {
                    mode.select_by_path(&path);
                } else {
                    mode.clamp_selection();
                }
            }
            _ => {}
        }
        self.mode = ReaderMode::Library(Box::new(mode));
        Vec::new()
    }

    fn library_tags_key(&mut self, mut mode: LibraryMode, key: InputKey) -> Vec<ReaderEffect> {
        let LibraryPhase::EditingTags {
            mut input,
            return_to_card,
        } = std::mem::replace(&mut mode.phase, LibraryPhase::Browsing)
        else {
            self.mode = ReaderMode::Library(Box::new(mode));
            return Vec::new();
        };
        match key {
            InputKey::Escape => {
                mode.phase = if return_to_card {
                    LibraryPhase::Card
                } else {
                    LibraryPhase::Browsing
                };
            }
            InputKey::Enter => {
                let Some(entry) = mode.selected_entry() else {
                    self.mode = ReaderMode::Library(Box::new(mode));
                    return Vec::new();
                };
                let path = entry.filepath.clone();
                let category = entry.category;
                let tags = parse_library_tags(&input);
                self.status = "Saving library tags…".into();
                return self.submit_library(
                    mode,
                    LibraryCommand::UpdateMetadata {
                        path,
                        category,
                        tags,
                    },
                    return_to_card,
                );
            }
            InputKey::Backspace | InputKey::Character('\u{7f}' | '\u{08}') => {
                pop_grapheme(&mut input);
                mode.phase = LibraryPhase::EditingTags {
                    input,
                    return_to_card,
                };
            }
            InputKey::Character('\u{15}') => {
                input.clear();
                mode.phase = LibraryPhase::EditingTags {
                    input,
                    return_to_card,
                };
            }
            InputKey::Character(ch) if !ch.is_control() => {
                input.push(ch);
                mode.phase = LibraryPhase::EditingTags {
                    input,
                    return_to_card,
                };
            }
            _ => {
                mode.phase = LibraryPhase::EditingTags {
                    input,
                    return_to_card,
                };
            }
        }
        self.mode = ReaderMode::Library(Box::new(mode));
        Vec::new()
    }

    fn library_category_key(
        &mut self,
        mut mode: LibraryMode,
        key: InputKey,
        command: Option<Command>,
    ) -> Vec<ReaderEffect> {
        let LibraryPhase::ChoosingCategory {
            mut selected,
            return_to_card,
        } = std::mem::replace(&mut mode.phase, LibraryPhase::Browsing)
        else {
            self.mode = ReaderMode::Library(Box::new(mode));
            return Vec::new();
        };
        let quick_pick = match key {
            InputKey::Character('1' | 'b' | 'B') => Some(0),
            InputKey::Character('2' | 'w' | 'W') => Some(1),
            InputKey::Character('3' | 'p' | 'P') => Some(2),
            InputKey::Character('0' | 'x' | 'X') | InputKey::Backspace => Some(3),
            _ => None,
        };
        if let Some(choice_index) = quick_pick {
            return self.apply_category_choice(mode, choice_index, return_to_card);
        }
        match key {
            InputKey::Escape | InputKey::Character('c') => {
                mode.phase = if return_to_card {
                    LibraryPhase::Card
                } else {
                    LibraryPhase::Browsing
                };
            }
            InputKey::Enter => {
                return self.apply_category_choice(mode, selected, return_to_card);
            }
            _ => match command {
                Some(Command::ScrollDown | Command::PageDown) => {
                    selected = (selected + 1).min(CATEGORY_CHOICES.len() - 1);
                    mode.phase = LibraryPhase::ChoosingCategory {
                        selected,
                        return_to_card,
                    };
                }
                Some(Command::ScrollUp | Command::PageUp) => {
                    selected = selected.saturating_sub(1);
                    mode.phase = LibraryPhase::ChoosingCategory {
                        selected,
                        return_to_card,
                    };
                }
                _ => {
                    mode.phase = LibraryPhase::ChoosingCategory {
                        selected,
                        return_to_card,
                    };
                }
            },
        }
        self.mode = ReaderMode::Library(Box::new(mode));
        Vec::new()
    }

    fn apply_category_choice(
        &mut self,
        mode: LibraryMode,
        choice_index: usize,
        return_to_card: bool,
    ) -> Vec<ReaderEffect> {
        let Some(entry) = mode.selected_entry() else {
            self.mode = ReaderMode::Library(Box::new(mode));
            return Vec::new();
        };
        let path = entry.filepath.clone();
        let tags = entry.tags.clone();
        let category = CATEGORY_CHOICES.get(choice_index).copied().flatten();
        self.status = "Saving library category…".into();
        self.submit_library(
            mode,
            LibraryCommand::UpdateMetadata {
                path,
                category,
                tags,
            },
            return_to_card,
        )
    }

    fn library_card_key(
        &mut self,
        mut mode: LibraryMode,
        key: InputKey,
        command: Option<Command>,
    ) -> Vec<ReaderEffect> {
        let visible_len = mode.visible_entries().len();
        match command {
            Some(Command::ScrollDown | Command::PageDown) => {
                mode.selected = mode
                    .selected
                    .saturating_add(1)
                    .min(visible_len.saturating_sub(1));
            }
            Some(Command::ScrollUp | Command::PageUp) => {
                mode.selected = mode.selected.saturating_sub(1);
            }
            _ if key == InputKey::Enter || command == Some(Command::Follow) => {
                return self.open_selected_library_entry(mode);
            }
            _ if key == InputKey::Character('t') => {
                if let Some(entry) = mode.selected_entry() {
                    let input = format_library_tags(&entry.tags);
                    mode.phase = LibraryPhase::EditingTags {
                        input,
                        return_to_card: true,
                    };
                }
            }
            _ if key == InputKey::Character('c') => {
                if let Some(entry) = mode.selected_entry() {
                    let selected = category_choice_index(entry.category);
                    mode.phase = LibraryPhase::ChoosingCategory {
                        selected,
                        return_to_card: true,
                    };
                }
            }
            _ if matches!(key, InputKey::Delete | InputKey::Character('d')) && visible_len > 0 => {
                mode.phase = LibraryPhase::ConfirmRemove {
                    return_to_card: true,
                };
            }
            _ => {}
        }
        self.mode = ReaderMode::Library(Box::new(mode));
        Vec::new()
    }

    fn library_browse_key(
        &mut self,
        mut mode: LibraryMode,
        key: InputKey,
        command: Option<Command>,
    ) -> Vec<ReaderEffect> {
        if let InputKey::Character(digit) = key
            && digit.is_ascii_digit()
        {
            if mode.prefix.len() < 7 {
                mode.prefix.push(digit);
            }
            self.mode = ReaderMode::Library(Box::new(mode));
            return Vec::new();
        }
        let count = mode.prefix.parse::<usize>().unwrap_or(1).max(1);
        mode.prefix.clear();
        let visible_len = mode.visible_entries().len();
        match command {
            Some(Command::ScrollDown | Command::PageDown) => {
                mode.selected = mode
                    .selected
                    .saturating_add(count)
                    .min(visible_len.saturating_sub(1))
            }
            Some(Command::ScrollUp | Command::PageUp) => {
                mode.selected = mode.selected.saturating_sub(count)
            }
            Some(Command::BeginningOfCh) => mode.selected = 0,
            Some(Command::EndOfCh) => mode.selected = visible_len.saturating_sub(1),
            _ if key == InputKey::Character('/') => {
                mode.phase = LibraryPhase::Searching;
            }
            _ if key == InputKey::Character('s') && visible_len > 0 => {
                mode.phase = LibraryPhase::Card;
            }
            _ if key == InputKey::Character('t') => {
                if let Some(entry) = mode.selected_entry() {
                    let input = format_library_tags(&entry.tags);
                    mode.phase = LibraryPhase::EditingTags {
                        input,
                        return_to_card: false,
                    };
                }
            }
            _ if key == InputKey::Character('c') => {
                if let Some(entry) = mode.selected_entry() {
                    let selected = category_choice_index(entry.category);
                    mode.phase = LibraryPhase::ChoosingCategory {
                        selected,
                        return_to_card: false,
                    };
                }
            }
            _ if key == InputKey::Enter || command == Some(Command::Follow) => {
                return self.open_selected_library_entry(mode);
            }
            _ if key == InputKey::Character('D') => return self.remove_library_entry(mode),
            _ if matches!(key, InputKey::Delete | InputKey::Character('d')) && visible_len > 0 => {
                mode.phase = LibraryPhase::ConfirmRemove {
                    return_to_card: false,
                };
            }
            _ => {}
        }
        self.mode = ReaderMode::Library(Box::new(mode));
        Vec::new()
    }

    fn open_selected_library_entry(&mut self, mode: LibraryMode) -> Vec<ReaderEffect> {
        if self.loading {
            self.queue_input(super::QueuedInput::Input(super::ModalInput::Key(
                InputKey::Enter,
            )));
            self.mode = ReaderMode::Library(Box::new(mode));
            return Vec::new();
        }
        if let Some(entry) = mode.selected_entry() {
            if entry.filepath == self.info.path {
                self.status.clear();
                return Vec::new();
            }
            let path = entry.filepath.clone();
            self.status = "Saving position before switching books…".into();
            return self.submit_library(
                mode,
                LibraryCommand::Switch {
                    path,
                    departure: self.save_request(false),
                },
                false,
            );
        }
        self.mode = ReaderMode::Library(Box::new(mode));
        Vec::new()
    }

    fn remove_library_entry(&mut self, mode: LibraryMode) -> Vec<ReaderEffect> {
        let Some(entry) = mode.selected_entry() else {
            self.mode = ReaderMode::Library(Box::new(mode));
            return Vec::new();
        };
        let path = entry.filepath.clone();
        self.status = "Removing saved position and bookmarks…".into();
        self.submit_library(mode, LibraryCommand::Remove(path), false)
    }

    pub fn library_completed(&mut self, result: LibraryResult) -> Vec<ReaderEffect> {
        if !matches!(&self.mode, ReaderMode::Library(mode) if matches!(&mode.phase, LibraryPhase::Waiting { request_id, .. } if *request_id == result.request_id))
        {
            return Vec::new();
        }
        let ReaderMode::Library(mut mode) = std::mem::replace(&mut self.mode, ReaderMode::Reading)
        else {
            return Vec::new();
        };
        let previous = std::mem::replace(&mut mode.phase, LibraryPhase::Browsing);
        let return_to_card = matches!(
            previous,
            LibraryPhase::Waiting {
                return_to_card: true,
                ..
            }
        );
        let mut effects = Vec::new();
        match result.reply {
            Ok(LibraryReply::Entries(entries)) => {
                mode.entries = entries;
                mode.selected = mode
                    .visible_entries()
                    .iter()
                    .position(|entry| entry.filepath == self.info.path)
                    .unwrap_or(0);
                self.status.clear();
            }
            Ok(LibraryReply::Removed { path, entries }) => {
                mode.entries = entries;
                let count = mode.visible_entries().len();
                mode.selected = mode.selected.saturating_sub(1).min(count.saturating_sub(1));
                if path == self.info.path {
                    Arc::make_mut(&mut self.info).bookmarks.clear();
                }
                self.status =
                    "Removed saved position and bookmarks. Ebook and annotations retained.".into();
                effects.push(ReaderEffect::ForgetBookSession(path));
            }
            Ok(LibraryReply::MetadataUpdated { path, entries }) => {
                mode.entries = entries;
                mode.select_by_path(&path);
                if return_to_card && mode.selected_entry().is_some() {
                    mode.phase = LibraryPhase::Card;
                }
                self.status = "Updated library entry.".into();
            }
            Ok(LibraryReply::SwitchReady(path)) => {
                if matches!(previous, LibraryPhase::Waiting { command: LibraryCommand::Switch { path: expected, .. }, .. } if expected == path)
                {
                    mode.phase = LibraryPhase::Opening(path.clone());
                    self.status = "Opening selected ebook… Escape cancels.".into();
                    effects.push(ReaderEffect::OpenBook(path));
                }
            }
            Err(error) => {
                self.status = format!("Library operation failed: {error}");
                mode.ignored_input |= !mode.queued.is_empty();
                mode.queued.clear();
            }
        }
        let ignored = mode.ignored_input;
        let queued = std::mem::take(&mut mode.queued);
        self.mode = ReaderMode::Library(mode);
        for input in queued {
            effects.extend(self.replay_library_input(input));
        }
        if ignored {
            if let ReaderMode::Library(mode) = &mut self.mode {
                if !matches!(mode.phase, LibraryPhase::Waiting { .. }) {
                    mode.ignored_input = false;
                    mode.prefix.clear();
                }
            } else {
                self.prefix.clear();
            }
            self.pending_input_overflow |= self.loading;
            self.append_input_warning();
        }
        effects
    }

    fn replay_library_input(&mut self, input: LibraryInput) -> Vec<ReaderEffect> {
        match input {
            LibraryInput::Key(key) => self.handle_key(key),
            LibraryInput::Motion { key, repetitions } => {
                self.replay_library_motion(key, repetitions)
            }
        }
    }

    fn replay_library_motion(&mut self, key: InputKey, repetitions: usize) -> Vec<ReaderEffect> {
        match &mut self.mode {
            ReaderMode::Library(mode) => match mode.phase {
                LibraryPhase::Waiting { .. } => {
                    if !mode.queue(LibraryInput::Motion { key, repetitions }) {
                        self.status = "Input queue full. New input was ignored.".into();
                    }
                    return Vec::new();
                }
                LibraryPhase::Browsing => {
                    mode.prefix = mode
                        .prefix
                        .parse::<usize>()
                        .unwrap_or(1)
                        .saturating_add(repetitions.saturating_sub(1))
                        .to_string();
                }
                LibraryPhase::Opening(_) => {}
                LibraryPhase::ConfirmRemove { .. }
                | LibraryPhase::Searching
                | LibraryPhase::Card
                | LibraryPhase::EditingTags { .. }
                | LibraryPhase::ChoosingCategory { .. }
                    if repetitions > 1 =>
                {
                    let mut effects = self.handle_key(key);
                    effects.extend(self.replay_library_input(LibraryInput::Motion {
                        key,
                        repetitions: repetitions - 1,
                    }));
                    return effects;
                }
                _ => {}
            },
            ReaderMode::Reading => {
                self.prefix = self
                    .prefix
                    .parse::<usize>()
                    .unwrap_or(1)
                    .saturating_add(repetitions.saturating_sub(1))
                    .to_string();
            }
            _ => {}
        }
        self.handle_key(key)
    }

    pub fn library_open_failed(&mut self, message: String) {
        if let ReaderMode::Library(mode) = &mut self.mode
            && matches!(mode.phase, LibraryPhase::Opening(_))
        {
            mode.phase = LibraryPhase::Browsing;
            self.status = format!("Cannot open ebook: {message}");
        }
    }
}
