//! The Tauri commands the UI calls. Each one goes through [`Uta`], so the UI
//! never changes project data itself.
//!
//! They're synchronous, so Tauri runs them in the order they're sent, except
//! `set_buffer_size`, which reopens the device and mustn't hold up the main
//! thread.

use std::sync::{Mutex, MutexGuard, PoisonError};

use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use crate::menu::EditMenu;
use crate::uta::{Frame, ProjectView, Uta};

/// The event sent with a [`ProjectView`] after every change to the project,
/// wherever it came from (a command or the menu).
pub const PROJECT_CHANGED: &str = "project-changed";

pub struct AppState {
    pub uta: Mutex<Uta>,
    /// Where the frame thread sends each [`Frame`], once the UI subscribes.
    pub frames: Mutex<Option<Channel<Frame>>>,
}

/// Locks `mutex`, carrying on if a panic poisoned it: the state inside is
/// still whole, because every change to it is a single step.
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Changes the project with `change`, then tells the menu and the UI.
pub fn edit<R: Runtime>(
    app: &AppHandle<R>,
    change: impl FnOnce(&mut Uta) -> Result<(), String>,
) -> Result<ProjectView, String> {
    let project = {
        let state = app.state::<AppState>();
        let mut uta = lock(&state.uta);
        change(&mut uta)?;
        uta.project()
    };
    if let Some(menu) = app.try_state::<EditMenu<R>>() {
        menu.update(&project).map_err(|error| error.to_string())?;
    }
    app.emit(PROJECT_CHANGED, &project)
        .map_err(|error| error.to_string())?;
    Ok(project)
}

#[tauri::command]
pub fn get_project(state: State<'_, AppState>) -> ProjectView {
    lock(&state.uta).project()
}

/// Sets the master volume. Calls with the same `gesture` (one drag) undo as
/// one step.
#[tauri::command]
pub fn set_volume<R: Runtime>(
    app: AppHandle<R>,
    volume_db: f32,
    gesture: Option<u32>,
) -> Result<ProjectView, String> {
    edit(&app, |uta| uta.set_volume(volume_db, gesture))
}

/// Sets the tempo, in BPM. Calls with the same `gesture` undo as one step.
#[tauri::command]
pub fn set_tempo<R: Runtime>(
    app: AppHandle<R>,
    bpm: f32,
    gesture: Option<u32>,
) -> Result<ProjectView, String> {
    edit(&app, |uta| uta.set_tempo(bpm, gesture))
}

/// Sets the loop's length, in bars. Calls with the same `gesture` undo as
/// one step.
#[tauri::command]
pub fn set_loop_length<R: Runtime>(
    app: AppHandle<R>,
    bars: u32,
    gesture: Option<u32>,
) -> Result<ProjectView, String> {
    edit(&app, |uta| uta.set_loop_length(bars, gesture))
}

/// Fills the loop with a few thousand notes, as one undo step. The Develop
/// menu calls it.
pub fn add_stress_notes<R: Runtime>(app: AppHandle<R>) -> Result<ProjectView, String> {
    edit(&app, Uta::add_stress_notes)
}

#[tauri::command]
pub fn undo<R: Runtime>(app: AppHandle<R>) -> Result<ProjectView, String> {
    edit(&app, |uta| {
        uta.undo();
        Ok(())
    })
}

#[tauri::command]
pub fn redo<R: Runtime>(app: AppHandle<R>) -> Result<ProjectView, String> {
    edit(&app, |uta| {
        uta.redo();
        Ok(())
    })
}

#[tauri::command]
pub fn play(state: State<'_, AppState>) -> Result<(), String> {
    lock(&state.uta).play()
}

#[tauri::command]
pub fn stop(state: State<'_, AppState>) -> Result<(), String> {
    lock(&state.uta).stop()
}

#[tauri::command]
pub async fn set_buffer_size(state: State<'_, AppState>, size: u32) -> Result<(), String> {
    lock(&state.uta).set_buffer_size(size)
}

/// Starts sending frames to `on_frame`, replacing any earlier subscriber
/// (after a reload, say).
#[tauri::command]
pub fn subscribe(state: State<'_, AppState>, on_frame: Channel<Frame>) {
    *lock(&state.frames) = Some(on_frame);
}
