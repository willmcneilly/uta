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

            let (menu, edit_menu) = menu::build(app.handle())?;
            app.set_menu(menu)?;
            app.manage(edit_menu);
            app.on_menu_event(|app, event| {
                let result = match event.id().as_ref() {
                    menu::UNDO => commands::undo(app.clone()),
                    menu::REDO => commands::redo(app.clone()),
                    menu::ADD_STRESS_NOTES => commands::add_stress_notes(app.clone()),
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
            commands::set_loop_length,
            commands::undo,
            commands::redo,
            commands::play,
            commands::stop,
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
