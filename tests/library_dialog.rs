#[path = "support/reader.rs"]
mod reader_support;

use ers::app::requests::{LibraryJob, LibraryReply, LibraryResult};
use ers::app::{ReaderEffect, ReaderMode, ReaderModel};
use ers::book::LibraryEntry;
use ers::configuration::InputKey;
use reader_support::reader;

fn entry(path: &str) -> LibraryEntry {
    LibraryEntry {
        last_read: "2026-01-01 00:00:00".into(),
        filepath: path.into(),
        title: Some(path.into()),
        author: Some("Fixture author".into()),
        reading_progress: Some(0.25),
        category: None,
        tags: Vec::new(),
    }
}

fn request(effects: Vec<ReaderEffect>) -> LibraryJob {
    effects
        .into_iter()
        .find_map(|effect| match effect {
            ReaderEffect::Library(job) => Some(job),
            _ => None,
        })
        .expect("a library operation")
}

fn entries(model: &ReaderModel) -> Vec<LibraryEntry> {
    vec![
        entry(&model.view().info.path),
        entry("/synthetic/other.epub"),
    ]
}

fn reply(job: &LibraryJob, reply: LibraryReply) -> LibraryResult {
    LibraryResult {
        request_id: job.request_id,
        reply: Ok(reply),
    }
}

fn browsing() -> ReaderModel {
    let mut model = reader();
    let rows = entries(&model);
    let job = request(model.handle_key(InputKey::Character('R')));
    model.library_completed(reply(&job, LibraryReply::Entries(rows)));
    model
}

#[test]
fn library_command_requests_a_durable_departure_position() {
    let mut model = reader();
    for key in "17j".chars() {
        assert!(model.handle_key(InputKey::Character(key)).is_empty());
    }
    let position = model.position();
    let job = request(model.handle_key(InputKey::Character('R')));
    assert_eq!(job.command.departure().unwrap().position, position);
    assert!(!model.can_autosave());
}

#[test]
fn pending_library_keys_preserve_selection_before_opening() {
    let mut model = reader();
    let rows = entries(&model);
    let job = request(model.handle_key(InputKey::Character('R')));
    model.handle_key(InputKey::Down);
    model.handle_key(InputKey::Enter);
    let switching = request(model.library_completed(reply(&job, LibraryReply::Entries(rows))));
    let effects = model.library_completed(reply(
        &switching,
        LibraryReply::SwitchReady("/synthetic/other.epub".into()),
    ));
    assert!(
        matches!(effects.as_slice(), [ReaderEffect::OpenBook(path)] if path == "/synthetic/other.epub")
    );
}

#[test]
fn overflowing_library_input_reports_ignored_keys_immediately() {
    let mut model = reader();
    request(model.handle_key(InputKey::Character('R')));
    for index in 0..129 {
        model.handle_key(if index % 2 == 0 {
            InputKey::Down
        } else {
            InputKey::Up
        });
    }
    assert!(model.view().status.contains("input was ignored"));
}

#[test]
fn overflowing_library_input_is_reported_after_results_arrive() {
    let mut model = reader();
    let rows = entries(&model);
    let job = request(model.handle_key(InputKey::Character('R')));
    for index in 0..129 {
        model.handle_key(if index % 2 == 0 {
            InputKey::Down
        } else {
            InputKey::Up
        });
    }
    model.library_completed(reply(&job, LibraryReply::Entries(rows)));
    assert!(model.view().status.contains("input was ignored"));
}

#[test]
fn rejected_library_movement_does_not_leave_a_numeric_prefix() {
    let mut model = reader();
    let mut rows = vec![entry(&model.view().info.path)];
    rows.extend((0..12).map(|index| entry(&format!("/synthetic/book{index}.epub"))));
    let destination = rows[2].filepath.clone();
    let job = request(model.handle_key(InputKey::Character('R')));
    for index in 0..127 {
        model.handle_key(if index % 2 == 0 {
            InputKey::Down
        } else {
            InputKey::Up
        });
    }
    model.handle_key(InputKey::Character('5'));
    model.handle_key(InputKey::Down);
    model.library_completed(reply(&job, LibraryReply::Entries(rows)));
    model.handle_key(InputKey::Down);
    let switching = request(model.handle_key(InputKey::Enter));
    assert!(
        matches!(switching.command, ers::app::requests::LibraryCommand::Switch { path, .. } if path == destination)
    );
}

#[test]
fn failed_library_operation_reports_discarded_queued_input() {
    let mut model = reader();
    let job = request(model.handle_key(InputKey::Character('R')));
    model.handle_key(InputKey::Down);
    model.library_completed(LibraryResult {
        request_id: job.request_id,
        reply: Err("disk full".into()),
    });
    assert!(model.view().status.contains("disk full"));
    assert!(model.view().status.contains("input was ignored"));
}

#[test]
fn canceled_library_requests_cannot_replace_a_later_dialog() {
    let mut model = reader();
    let rows = entries(&model);
    let job = request(model.handle_key(InputKey::Character('R')));
    model.handle_key(InputKey::Escape);
    model.handle_key(InputKey::Character('?'));
    let title = match model.view().mode {
        ReaderMode::Dialog(dialog) => dialog.title.clone(),
        _ => panic!("Help must be open before the stale reply"),
    };
    assert!(
        model
            .library_completed(reply(&job, LibraryReply::Entries(rows)))
            .is_empty()
    );
    assert!(matches!(model.view().mode, ReaderMode::Dialog(dialog) if dialog.title == title));
}

#[test]
fn held_library_navigation_preserves_the_final_selection() {
    let mut model = reader();
    let mut rows = vec![entry(&model.view().info.path)];
    rows.extend((0..1000).map(|index| entry(&format!("/synthetic/book{index}.epub"))));
    let destination = rows[600].filepath.clone();
    let job = request(model.handle_key(InputKey::Character('R')));
    for _ in 0..600 {
        model.handle_key(InputKey::Down);
    }
    model.handle_key(InputKey::Enter);
    let switching = request(model.library_completed(reply(&job, LibraryReply::Entries(rows))));
    assert!(
        matches!(switching.command, ers::app::requests::LibraryCommand::Switch { path, .. } if path == destination)
    );
}

#[test]
fn current_book_selection_closes_without_reopening() {
    let mut model = browsing();
    assert!(model.handle_key(InputKey::Enter).is_empty());
    assert!(matches!(model.view().mode, ReaderMode::Reading));
}

#[test]
fn failed_departure_save_retains_the_library_selection() {
    let mut model = browsing();
    model.handle_key(InputKey::Down);
    let job = request(model.handle_key(InputKey::Enter));
    let effects = model.library_completed(LibraryResult {
        request_id: job.request_id,
        reply: Err("disk full".into()),
    });
    assert!(effects.is_empty());
    assert!(model.view().status.contains("disk full"));
    let retry = request(model.handle_key(InputKey::Enter));
    let effects = model.library_completed(reply(
        &retry,
        LibraryReply::SwitchReady("/synthetic/other.epub".into()),
    ));
    assert!(
        matches!(effects.as_slice(), [ReaderEffect::OpenBook(path)] if path == "/synthetic/other.epub")
    );
}

#[test]
fn uppercase_library_removal_does_not_require_confirmation() {
    let mut model = browsing();
    model.handle_key(InputKey::Down);
    let job = request(model.handle_key(InputKey::Character('D')));
    assert!(
        matches!(job.command, ers::app::requests::LibraryCommand::Remove(path) if path == "/synthetic/other.epub")
    );
}

#[test]
fn removing_current_history_retains_session_quickmarks() {
    let mut model = reader();
    for _ in 0..5 {
        model.handle_key(InputKey::Down);
    }
    model.handle_key(InputKey::Character('m'));
    model.handle_key(InputKey::Character('1'));
    let marked = model.view_position();
    let rows = entries(&model);
    let job = request(model.handle_key(InputKey::Character('R')));
    model.library_completed(reply(&job, LibraryReply::Entries(rows)));
    model.handle_key(InputKey::Character('d'));
    let removal = request(model.handle_key(InputKey::Character('y')));
    model.library_completed(reply(
        &removal,
        LibraryReply::Removed {
            path: model.view().info.path.clone(),
            entries: vec![entry("/synthetic/other.epub")],
        },
    ));
    model.handle_key(InputKey::Escape);
    model.handle_key(InputKey::Down);
    model.handle_key(InputKey::Character('`'));
    let effects = model.handle_key(InputKey::Character('1'));
    assert!(
        matches!(effects.as_slice(), [ReaderEffect::Load(request)] if request.position.as_ref() == Some(&marked))
    );
}

#[test]
fn pasted_confirmation_cannot_remove_a_library_entry() {
    let mut model = browsing();
    model.handle_key(InputKey::Down);
    model.handle_key(InputKey::Character('d'));
    model.handle_paste("y\n");
    let effects = model.handle_key(InputKey::Character('n'));
    assert!(effects.is_empty());
    let job = request(model.handle_key(InputKey::Enter));
    let effects = model.library_completed(reply(
        &job,
        LibraryReply::SwitchReady("/synthetic/other.epub".into()),
    ));
    assert!(matches!(effects.as_slice(), [ReaderEffect::OpenBook(_)]));
}

#[test]
fn canceled_opening_keeps_the_library_available() {
    let mut model = browsing();
    model.handle_key(InputKey::Down);
    let job = request(model.handle_key(InputKey::Enter));
    model.library_completed(reply(
        &job,
        LibraryReply::SwitchReady("/synthetic/other.epub".into()),
    ));
    assert!(matches!(
        model.handle_key(InputKey::Escape).as_slice(),
        [ReaderEffect::CancelBookOpen]
    ));
    assert!(matches!(
        request(model.handle_key(InputKey::Enter)).command,
        ers::app::requests::LibraryCommand::Switch { .. }
    ));
}

#[test]
fn failed_opening_preserves_the_departing_position() {
    let mut model = browsing();
    for key in "21j".chars() {
        assert!(model.handle_key(InputKey::Character(key)).is_empty());
    }
    let position = model.position();
    model.handle_key(InputKey::Down);
    let job = request(model.handle_key(InputKey::Enter));
    model.library_completed(reply(
        &job,
        LibraryReply::SwitchReady("/synthetic/other.epub".into()),
    ));
    model.library_open_failed("Unreadable ebook".into());
    assert_eq!(model.position(), position);
    assert!(model.view().status.contains("Unreadable ebook"));
    assert!(matches!(
        request(model.handle_key(InputKey::Enter)).command,
        ers::app::requests::LibraryCommand::Switch { .. }
    ));
}

#[test]
fn library_search_filters_by_title_category_and_hash_tags() {
    use ers::app::modes::library::LibraryPhase;
    use ers::book::LibraryCategory;

    let mut model = reader();
    let mut paper = entry("https://arxiv.org/html/2609.21032v1");
    paper.title = Some("Scaling Discovery".into());
    paper.author = Some("Jongho Park".into());
    paper.category = Some(LibraryCategory::Paper);
    paper.tags = vec!["agents".into(), "ml".into()];

    let mut book = entry("/books/three-body.epub");
    book.title = Some("The Three-Body Problem".into());
    book.author = Some("Cixin Liu".into());
    book.category = Some(LibraryCategory::Book);
    book.tags = vec!["scifi".into()];

    let job = request(model.handle_key(InputKey::Character('R')));
    model.library_completed(reply(
        &job,
        LibraryReply::Entries(vec![paper.clone(), book]),
    ));

    model.handle_key(InputKey::Character('/'));
    assert!(matches!(
        model.view().mode,
        ReaderMode::Library(mode) if matches!(mode.phase, LibraryPhase::Searching)
    ));
    for ch in "paper #ml".chars() {
        model.handle_key(InputKey::Character(ch));
    }
    model.handle_key(InputKey::Enter);

    let ReaderMode::Library(mode) = model.view().mode else {
        panic!("library must stay open");
    };
    assert!(matches!(mode.phase, LibraryPhase::Browsing));
    assert_eq!(mode.query, "paper #ml");
    assert_eq!(mode.visible_entries().len(), 1);
    assert_eq!(mode.selected_entry().unwrap().filepath, paper.filepath);

    // Escape in browsing clears the active search filter before closing.
    model.handle_key(InputKey::Escape);
    let ReaderMode::Library(mode) = model.view().mode else {
        panic!("clearing query must keep library open");
    };
    assert!(mode.query.is_empty());
    assert_eq!(mode.visible_entries().len(), 2);
}

#[test]
fn library_card_tags_and_category_round_trip() {
    use ers::app::modes::library::LibraryPhase;
    use ers::app::requests::LibraryCommand;
    use ers::book::LibraryCategory;

    let mut model = browsing();
    model.handle_key(InputKey::Down);
    model.handle_key(InputKey::Character('s'));
    assert!(matches!(
        model.view().mode,
        ReaderMode::Library(mode) if matches!(mode.phase, LibraryPhase::Card)
    ));

    // Edit tags from card view and verify it returns to card after update.
    model.handle_key(InputKey::Character('t'));
    for ch in "#rust, reading".chars() {
        model.handle_key(InputKey::Character(ch));
    }
    let tag_job = request(model.handle_key(InputKey::Enter));
    assert!(matches!(
        &tag_job.command,
        LibraryCommand::UpdateMetadata { path, category: None, tags }
            if path == "/synthetic/other.epub" && tags == &vec!["rust".to_owned(), "reading".to_owned()]
    ));

    let mut updated_other = entry("/synthetic/other.epub");
    updated_other.tags = vec!["rust".into(), "reading".into()];
    model.library_completed(reply(
        &tag_job,
        LibraryReply::MetadataUpdated {
            path: "/synthetic/other.epub".into(),
            entries: vec![entry(&model.view().info.path), updated_other.clone()],
        },
    ));
    assert!(matches!(
        model.view().mode,
        ReaderMode::Library(mode) if matches!(mode.phase, LibraryPhase::Card)
    ));

    // Choose category 'webpage' (quick key 'w') from card view.
    model.handle_key(InputKey::Character('c'));
    let cat_job = request(model.handle_key(InputKey::Character('w')));
    assert!(matches!(
        &cat_job.command,
        LibraryCommand::UpdateMetadata {
            path,
            category: Some(LibraryCategory::Webpage),
            tags
        } if path == "/synthetic/other.epub" && tags == &vec!["rust".to_owned(), "reading".to_owned()]
    ));

    // Closing card with 's' returns to browsing.
    updated_other.category = Some(LibraryCategory::Webpage);
    model.library_completed(reply(
        &cat_job,
        LibraryReply::MetadataUpdated {
            path: "/synthetic/other.epub".into(),
            entries: vec![entry(&model.view().info.path), updated_other],
        },
    ));
    model.handle_key(InputKey::Character('s'));
    assert!(matches!(
        model.view().mode,
        ReaderMode::Library(mode) if matches!(mode.phase, LibraryPhase::Browsing)
    ));
}
