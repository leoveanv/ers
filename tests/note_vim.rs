use ers::app::editor::{EditorAction, EditorOptions, EditorState};
use ers::configuration::InputKey;

fn note(text: &str) -> EditorState {
    EditorState::new(text, EditorOptions::default()).with_vim()
}

fn keys(editor: &mut EditorState, commands: &str) {
    for key in commands.chars() {
        editor.handle(InputKey::from_character(key));
    }
}

#[test]
fn escape_keeps_the_draft_open() {
    let mut editor = note("draft");
    assert_eq!(editor.handle(InputKey::Escape), None);
    assert_eq!(editor.handle(InputKey::Escape), None);
    assert_eq!(editor.mode_label(), "NORMAL");
    keys(&mut editor, "\t\t\n");
    assert_eq!(editor.handle(InputKey::Enter), Some(EditorAction::Cancel));
}

#[test]
fn normal_commands_edit_at_expected_positions() {
    for (text, commands, expected) in [
        ("one two!", "0wix", "one xtwo!"),
        ("one two!", "0eix", "onxe two!"),
        ("one two!", "bix", "one xtwo!"),
        ("abc", "0la!", "ab!c"),
        ("abc", "0llhix", "axbc"),
        ("  abc", "Ix", "  xabc"),
        ("abc", "0Ax", "abcx"),
        ("abc", "0$ix", "abxc"),
        ("abc", "ox", "abc\nx"),
        ("abc", "Ox", "x\nabc"),
        ("a\nb\nc", "kdd", "a\nc"),
        ("a\nb", "dd", "a"),
        ("abc", "dd", ""),
        ("é👩‍🔬z", "0x", "👩‍🔬z"),
        ("a\nb\nc", "kkjx", "a\n\nc"),
        ("abc", "dix", "abxc"),
    ] {
        let mut editor = note(text);
        keys(&mut editor, "\u{1b}");
        keys(&mut editor, commands);
        assert_eq!(editor.text(), expected, "commands: {commands}");
    }
}

#[test]
fn undo_restores_an_entire_insert_session() {
    let mut editor = note("a");
    keys(&mut editor, "bc\u{1b}u");
    assert_eq!(editor.text(), "a");
    keys(&mut editor, "\u{12}");
    assert_eq!(editor.text(), "abc");
    keys(&mut editor, "xu");
    assert_eq!(editor.text(), "abc");
    keys(&mut editor, "uAx\u{1b}\u{12}");
    assert_eq!(editor.text(), "ax");
}

#[test]
fn paste_in_normal_mode_is_literal_and_undoable() {
    let mut editor = note("");
    keys(&mut editor, "\u{1b}");
    editor.insert("dd\n👩‍🔬");
    assert_eq!(editor.text(), "dd\n👩‍🔬");
    keys(&mut editor, "u");
    assert_eq!(editor.text(), "");
}

#[test]
fn ordinary_fields_still_cancel_on_escape() {
    let mut editor = EditorState::new("query", EditorOptions::default());
    assert_eq!(editor.handle(InputKey::Escape), Some(EditorAction::Cancel));
}
