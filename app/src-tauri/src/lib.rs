//! The Uta app: the window, wired to the project core and the engine. See
//! RFC-001, "The UI draws on canvas and receives streamed updates".

mod commands;
mod menu;
mod stress;
mod uta;

use std::sync::Mutex;

use tauri::{AppHandle, Manager, RunEvent};
use uta_engine::live;

use crate::commands::{AppState, lock};
use crate::uta::{FRAME_INTERVAL, Uta};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let uta = Uta::start(live::DEFAULT_BUFFER_SIZE)?;
            app.manage(AppState {
                uta: Mutex::new(uta),
                frames: Mutex::new(None),
            });

            let (menu, menu_state) = menu::build(app.handle())?;
            app.set_menu(menu)?;
            app.manage(menu_state);
            app.on_menu_event(|app, event| {
                let result = match event.id().as_ref() {
                    menu::UNDO => commands::undo(app.clone()).map(drop),
                    menu::REDO => commands::redo(app.clone()).map(drop),
                    id @ (menu::COPY | menu::PASTE | menu::DUPLICATE) => {
                        commands::pass_menu_item(app, commands::EDIT_MENU, id)
                    }
                    id @ (menu::ADD_TRACK | menu::DELETE_TRACK | menu::DUPLICATE_TRACK) => {
                        commands::pass_menu_item(app, commands::TRACK_MENU, id)
                    }
                    id @ menu::ADD_STRESS_NOTES => {
                        commands::pass_menu_item(app, commands::DEVELOP_MENU, id)
                    }
                    _ => return,
                };
                if let Err(error) = result {
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
            commands::set_volume,
            commands::set_tempo,
            commands::set_loop,
            commands::set_loop_enabled,
            commands::set_synth_param,
            commands::set_track_mixer,
            commands::solo_track_alone,
            commands::add_track,
            commands::duplicate_track,
            commands::remove_track,
            commands::move_track,
            commands::add_clip,
            commands::set_clips,
            commands::remove_clips,
            commands::add_stress_notes,
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
