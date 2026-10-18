//! The Uta app: the window, wired to the project core and the engine. See
//! RFC-001, "The UI draws on canvas and receives streamed updates".

mod commands;
mod demos;
mod menu;
mod pieces;
mod stress;
mod uta;
mod window;

use std::sync::Mutex;

use tauri::{AppHandle, Manager, RunEvent, Runtime};
use uta_engine::live;

use crate::commands::{AppState, lock};
use crate::uta::{FRAME_INTERVAL, Uta};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(window::state_flags())
                // `window::place` decides whether to restore.
                .skip_initial_state(window::MAIN)
                .build(),
        )
        .setup(|app| {
            window::place(app)?;

            let uta = Uta::start(live::DEFAULT_BUFFER_SIZE)?;
            app.manage(AppState {
                uta: Mutex::new(uta),
                frames: Mutex::new(None),
            });

            let (menu, menu_state) = menu::build(app.handle())?;
            app.set_menu(menu)?;
            app.manage(menu_state);
            app.on_menu_event(|app, event| {
                if let Err(error) = menu_item(app, event.id().as_ref()) {
                    eprintln!("uta: {error}");
                }
            });

            std::thread::Builder::new()
                .name("uta-frames".into())
                .spawn({
                    let app = app.handle().clone();
                    move || send_frames(&app)
                })?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_project,
            commands::get_notes,
            commands::set_volume,
            commands::set_tempo,
            commands::set_loop,
            commands::set_loop_enabled,
            commands::set_synth_param,
            commands::set_synth_sound,
            commands::set_drum_param,
            commands::set_track_mixer,
            commands::solo_track_alone,
            commands::add_track,
            commands::duplicate_track,
            commands::remove_track,
            commands::move_track,
            commands::add_effect,
            commands::remove_effects,
            commands::move_effect,
            commands::set_effect_enabled,
            commands::set_effect_mix,
            commands::set_delay_param,
            commands::add_clip,
            commands::set_clips,
            commands::paste_clips,
            commands::remove_clips,
            commands::add_stress_notes,
            commands::build_test_song,
            commands::load_demo_song,
            commands::add_notes,
            commands::set_notes,
            commands::remove_notes,
            commands::trim_notes,
            commands::cancel_gesture,
            commands::audition_note,
            commands::undo,
            commands::redo,
            commands::play,
            commands::stop,
            commands::pause,
            commands::resume,
            commands::locate,
            commands::set_buffer_size,
            commands::subscribe,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                lock(&app.state::<AppState>().uta).shut_down();
            }
        });
}

/// Acts on the menu item `id`. Undo and Redo change the project here; the
/// UI didn't ask for them, so it's told they happened. The rest are passed
/// on to the UI, which sends back an ordinary command.
fn menu_item<R: Runtime>(app: &AppHandle<R>, id: &str) -> Result<(), String> {
    match id {
        menu::UNDO => commands::change_unasked(app, |uta| {
            uta.undo();
            Ok(())
        }),
        menu::REDO => commands::change_unasked(app, |uta| {
            uta.redo();
            Ok(())
        }),
        menu::COPY | menu::PASTE | menu::DUPLICATE => {
            commands::pass_menu_item(app, commands::EDIT_MENU, id)
        }
        menu::ADD_SYNTH_TRACK
        | menu::ADD_DRUM_TRACK
        | menu::DELETE_TRACK
        | menu::DUPLICATE_TRACK => commands::pass_menu_item(app, commands::TRACK_MENU, id),
        menu::ADD_STRESS_NOTES | menu::RUN_BENCHMARK => {
            commands::pass_menu_item(app, commands::DEVELOP_MENU, id)
        }
        _ => match menu::demo_song(id) {
            Some(song) => commands::pass_demo_song(app, song),
            None => Ok(()),
        },
    }
}

/// Sends the UI one [`uta::Frame`] per screen frame: the level, playhead,
/// dropouts and output, batched since the last one. Runs for the life of the
/// app. It polls the engine even with no subscriber, so used snapshots are
/// still freed.
fn send_frames(app: &AppHandle) {
    let state = app.state::<AppState>();
    loop {
        std::thread::sleep(FRAME_INTERVAL);
        let frame = lock(&state.uta).frame();
        let mut frames = lock(&state.frames);
        if let Some(channel) = frames.as_ref()
            && channel.send(frame).is_err()
        {
            // The page went away. It subscribes again when it loads.
            *frames = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};
    use tauri::{App, Listener};

    use super::*;
    use crate::uta::{TrackKind, Uta};

    /// The app on Tauri's mock runtime, with no sound device, recording
    /// every `project-changed` payload it emits. It has no menu bar: macOS
    /// only builds menus on the main thread, and tests run on others.
    fn app() -> (App<MockRuntime>, Arc<Mutex<Vec<String>>>) {
        let app = mock_builder()
            .build(mock_context(noop_assets()))
            .expect("the mock app builds");
        app.manage(AppState {
            uta: Mutex::new(Uta::offline()),
            frames: Mutex::new(None),
        });
        let events = Arc::new(Mutex::new(Vec::new()));
        app.listen_any(commands::PROJECT_CHANGED, {
            let events = events.clone();
            move |event| lock(&events).push(event.payload().to_owned())
        });
        (app, events)
    }

    #[test]
    fn a_command_replies_with_the_change_and_emits_no_event() {
        let (app, events) = app();
        let view = commands::set_volume(app.handle().clone(), -6.0, None).unwrap();
        assert_eq!(view.outline.volume_db, -6.0);
        let id = uta_core::TrackId::random();
        let view = commands::add_track(app.handle().clone(), id, TrackKind::Drums).unwrap();
        assert!(view.outline.tracks.iter().any(|track| track.id == id));
        assert!(lock(&events).is_empty());
    }

    #[test]
    fn undo_and_redo_from_the_menu_emit_project_changed_with_no_payload() {
        let (app, events) = app();
        commands::set_volume(app.handle().clone(), -6.0, None).unwrap();

        menu_item(app.handle(), menu::UNDO).unwrap();
        assert_eq!(*lock(&events), ["null"]);
        assert_ne!(commands::get_project(app.state()).outline.volume_db, -6.0);

        menu_item(app.handle(), menu::REDO).unwrap();
        assert_eq!(*lock(&events), ["null", "null"]);
        assert_eq!(commands::get_project(app.state()).outline.volume_db, -6.0);
    }

    #[test]
    fn notes_undone_from_the_menu_arrive_with_the_fetch_that_follows() {
        let (app, _) = app();
        let clip = commands::get_project(app.state()).outline.tracks[0].clips[0].id;
        let update = commands::add_stress_notes(app.handle().clone(), clip).unwrap();
        assert_eq!(update.notes.len(), 1);

        menu_item(app.handle(), menu::UNDO).unwrap();
        let update = commands::get_project(app.state());
        assert_eq!(update.notes.len(), 1, "the undone clip's notes are sent");
        assert_eq!(update.notes[0].clip, clip);
        assert!(update.notes[0].notes.is_empty());
        assert_eq!(
            update.outline.tracks[0].clips[0].notes_revision,
            update.notes[0].revision
        );
    }

    #[test]
    fn a_demo_song_from_the_menu_asks_the_ui_and_changes_nothing_itself() {
        let (app, events) = app();
        let asked = Arc::new(Mutex::new(Vec::new()));
        app.listen_any(commands::DEMO_SONG_MENU, {
            let asked = asked.clone();
            move |event| lock(&asked).push(event.payload().to_owned())
        });
        let before = commands::get_project(app.state()).outline;

        menu_item(app.handle(), &format!("{}beat", menu::DEMO_SONG)).unwrap();
        assert_eq!(*lock(&asked), [r#"{"id":"beat","name":"Beat"}"#]);
        assert_eq!(
            commands::get_project(app.state()).outline.tracks,
            before.tracks
        );
        assert!(lock(&events).is_empty());

        let update = commands::load_demo_song(app.handle().clone(), "beat".into()).unwrap();
        assert_eq!(update.outline.tracks.len(), 1);
        assert_eq!(update.outline.tracks[0].name, "Beat");
        assert!(lock(&events).is_empty(), "the reply carries it");
    }
}
