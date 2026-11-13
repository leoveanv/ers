use super::EditorState;
use crate::configuration::InputKey;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug)]
struct Snapshot {
    text: String,
    cursor_byte: usize,
}

#[derive(Clone, Debug, Default)]
pub(super) struct VimState {
    normal: bool,
    pending_delete: bool,
    insert_recorded: bool,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl EditorState {
    pub fn with_vim(mut self) -> Self {
        self.vim = Some(VimState::default());
        self
    }

    pub fn mode_label(&self) -> &'static str {
        if self.vim.as_ref().is_some_and(|vim| vim.normal) {
            "NORMAL"
        } else {
            "INSERT"
        }
    }

    pub(super) fn handle_vim(&mut self, key: InputKey) -> bool {
        let Some(vim) = self.vim.as_mut() else {
            return false;
        };
        if key == InputKey::Escape {
            let leaving_insert = !vim.normal;
            vim.normal = true;
            vim.pending_delete = false;
            vim.insert_recorded = false;
            self.focus = 0;
            if leaving_insert && self.cursor_byte > self.line_start() {
                self.cursor_byte = self.previous_boundary();
            }
            self.clamp_normal_cursor();
            return true;
        }
        if matches!(key, InputKey::Tab | InputKey::BackTab) || self.focus > 0 {
            vim.pending_delete = false;
            return false;
        }
        if !vim.normal {
            return false;
        }
        let delete = std::mem::take(&mut vim.pending_delete);
        if delete && key == InputKey::Character('d') {
            self.delete_line();
        } else {
            self.normal_key(key);
        }
        if self.vim.as_ref().is_some_and(|vim| vim.normal) {
            self.clamp_normal_cursor();
        }
        true
    }

    fn normal_key(&mut self, key: InputKey) {
        match key {
            InputKey::Character('h') | InputKey::Left => {
                self.cursor_byte = self.previous_boundary().max(self.line_start());
            }
            InputKey::Character('l') | InputKey::Right => {
                self.cursor_byte = self.next_boundary().min(self.line_end());
            }
            InputKey::Character('j') | InputKey::Down => self.move_vertical(true),
            InputKey::Character('k') | InputKey::Up => self.move_vertical(false),
            InputKey::Character('0') | InputKey::Home => self.cursor_byte = self.line_start(),
            InputKey::Character('$') | InputKey::End => self.cursor_byte = self.line_end(),
            InputKey::Character(motion @ ('w' | 'b' | 'e')) => self.move_word(motion),
            InputKey::Character(command @ ('i' | 'a' | 'I' | 'A' | 'o' | 'O')) => {
                self.start_insert(command);
            }
            InputKey::Character('x') | InputKey::Delete => {
                if self.cursor_byte < self.line_end() {
                    self.replace(self.cursor_byte, self.next_boundary(), "");
                }
            }
            InputKey::Character('d') => self.vim.as_mut().unwrap().pending_delete = true,
            InputKey::Character('u') => self.restore_history(false),
            InputKey::Character('\u{12}') => self.restore_history(true),
            _ => {}
        }
    }

    fn line_start(&self) -> usize {
        self.text[..self.cursor_byte]
            .rfind('\n')
            .map_or(0, |index| index + 1)
    }

    fn line_end(&self) -> usize {
        self.text[self.cursor_byte..]
            .find('\n')
            .map_or(self.text.len(), |index| self.cursor_byte + index)
    }

    fn clamp_normal_cursor(&mut self) {
        if self.cursor_byte == self.line_end() && self.cursor_byte > self.line_start() {
            self.cursor_byte = self.previous_boundary();
        }
    }

    fn start_insert(&mut self, command: char) {
        let vim = self.vim.as_mut().unwrap();
        vim.normal = false;
        vim.insert_recorded = false;
        match command {
            'a' => self.cursor_byte = self.next_boundary().min(self.line_end()),
            'A' | 'o' => self.cursor_byte = self.line_end(),
            'I' => {
                self.cursor_byte = self.line_start();
                while self.cursor_byte < self.line_end()
                    && self.text[self.cursor_byte..].starts_with([' ', '\t'])
                {
                    self.cursor_byte = self.next_boundary();
                }
            }
            'O' => self.cursor_byte = self.line_start(),
            _ => {}
        }
        if matches!(command, 'o' | 'O') {
            self.insert("\n");
            if command == 'O' {
                self.cursor_byte = self.previous_boundary();
            }
        }
    }

    fn delete_line(&mut self) {
        let start = self.line_start();
        let end = self.line_end();
        if end < self.text.len() {
            self.replace(start, end + 1, "");
        } else {
            self.replace(start.saturating_sub(1), end, "");
            self.cursor_byte = self.line_start();
        }
    }

    fn move_word(&mut self, motion: char) {
        let last = self.boundaries.len() - 1;
        let mut index = self
            .boundaries
            .partition_point(|byte| *byte < self.cursor_byte);
        if motion == 'b' {
            index = index.saturating_sub(1);
            while index > 0 && self.word_class(index) == 0 {
                index -= 1;
            }
            while index > 0 && self.word_class(index - 1) == self.word_class(index) {
                index -= 1;
            }
        } else if motion == 'w' {
            let class = self.word_class(index);
            while index < last && self.word_class(index) == class {
                index += 1;
            }
            while index < last && self.word_class(index) == 0 {
                index += 1;
            }
        } else {
            index = (index + 1).min(last);
            while index < last && self.word_class(index) == 0 {
                index += 1;
            }
            while index + 1 < last && self.word_class(index + 1) == self.word_class(index) {
                index += 1;
            }
        }
        self.cursor_byte = self.boundaries[index];
    }

    fn word_class(&self, index: usize) -> u8 {
        let Some(character) = self.text[self.boundaries[index]..].chars().next() else {
            return 0;
        };
        if character.is_whitespace() {
            0
        } else if character.is_alphanumeric() || character == '_' {
            1
        } else {
            2
        }
    }

    pub(super) fn record_change(&mut self) {
        let Some(vim) = self.vim.as_mut() else {
            return;
        };
        if !vim.normal && vim.insert_recorded {
            return;
        }
        vim.undo.push(Snapshot {
            text: self.text.clone(),
            cursor_byte: self.cursor_byte,
        });
        vim.redo.clear();
        vim.insert_recorded = !vim.normal;
    }

    fn restore_history(&mut self, redo: bool) {
        let vim = self.vim.as_mut().unwrap();
        let (source, destination) = if redo {
            (&mut vim.redo, &mut vim.undo)
        } else {
            (&mut vim.undo, &mut vim.redo)
        };
        let Some(snapshot) = source.pop() else {
            return;
        };
        destination.push(Snapshot {
            text: self.text.clone(),
            cursor_byte: self.cursor_byte,
        });
        self.text = snapshot.text;
        self.cursor_byte = snapshot.cursor_byte;
        self.boundaries = std::iter::once(0)
            .chain(
                self.text
                    .grapheme_indices(true)
                    .map(|(start, cluster)| start + cluster.len()),
            )
            .collect();
    }
}
