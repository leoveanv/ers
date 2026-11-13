use crate::app::mouse::{MouseAction, MouseClick, MouseContext};
use crate::app::{BookInfo, ReaderModel};
use crate::configuration::InputKey;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Size;
use std::sync::Arc;

pub(super) fn input_key(event: KeyEvent) -> Option<InputKey> {
    match event.code {
        KeyCode::Char(character) if event.modifiers.contains(KeyModifiers::CONTROL) => {
            if character.eq_ignore_ascii_case(&'i') {
                Some(InputKey::ControlI)
            } else if character.is_ascii_alphabetic() {
                Some(InputKey::Character(
                    (character.to_ascii_lowercase() as u8 - b'a' + 1) as char,
                ))
            } else {
                Some(InputKey::Character(character))
            }
        }
        KeyCode::Char(character) => Some(InputKey::from_character(character)),
        KeyCode::Up => Some(InputKey::Up),
        KeyCode::Down => Some(InputKey::Down),
        KeyCode::Left => Some(InputKey::Left),
        KeyCode::Right => Some(InputKey::Right),
        KeyCode::PageUp => Some(InputKey::PageUp),
        KeyCode::PageDown => Some(InputKey::PageDown),
        KeyCode::Home => Some(InputKey::Home),
        KeyCode::End => Some(InputKey::End),
        KeyCode::Tab => Some(InputKey::Tab),
        KeyCode::Enter => Some(InputKey::Enter),
        KeyCode::Esc => Some(InputKey::Escape),
        KeyCode::Backspace => Some(InputKey::Backspace),
        KeyCode::Delete => Some(InputKey::Delete),
        KeyCode::BackTab => Some(InputKey::BackTab),
        _ => None,
    }
}

struct MousePress {
    event: MouseEvent,
    context: MouseContext,
    book: Arc<BookInfo>,
}

#[derive(Default)]
pub(super) struct MouseTracker {
    pressed: Option<MousePress>,
}

impl MouseTracker {
    pub(super) fn cancel(&mut self) {
        self.pressed = None;
    }

    pub(super) fn input(
        &mut self,
        event: MouseEvent,
        screen: Size,
        model: Option<&ReaderModel>,
    ) -> Option<MouseAction> {
        match event.kind {
            MouseEventKind::ScrollUp if event.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(MouseAction::Enlarge)
            }
            MouseEventKind::ScrollDown if event.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(MouseAction::Shrink)
            }
            MouseEventKind::ScrollUp => Some(MouseAction::ScrollUp),
            MouseEventKind::ScrollDown => Some(MouseAction::ScrollDown),
            MouseEventKind::Down(MouseButton::Left | MouseButton::Right) => {
                self.pressed = model.map(|model| MousePress {
                    event,
                    context: MouseContext::from(model.view().mode),
                    book: model.view().info.clone(),
                });
                None
            }
            MouseEventKind::Down(_) => {
                self.cancel();
                None
            }
            MouseEventKind::Up(button) => self.release(button, event, screen, model),
            MouseEventKind::Drag(_) | MouseEventKind::Moved => {
                self.cancel();
                None
            }
            _ => None,
        }
    }

    fn release(
        &mut self,
        button: MouseButton,
        event: MouseEvent,
        screen: Size,
        model: Option<&ReaderModel>,
    ) -> Option<MouseAction> {
        let press = self.pressed.take()?;
        let model = model?;
        if press.event.kind != MouseEventKind::Down(button)
            || press.event.column != event.column
            || press.event.row != event.row
            || !press.event.modifiers.is_empty()
            || !event.modifiers.is_empty()
            || press.context != MouseContext::from(model.view().mode)
            || !Arc::ptr_eq(&press.book, model.view().info)
        {
            return None;
        }
        match button {
            MouseButton::Left => Some(MouseAction::LeftClick(MouseClick {
                column: event.column,
                row: event.row,
                screen_columns: screen.width,
                screen_rows: screen.height,
            })),
            MouseButton::Right => Some(MouseAction::RightClick),
            MouseButton::Middle => None,
        }
    }
}
