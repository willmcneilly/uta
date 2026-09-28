//! The Tauri commands the UI calls. Each one goes through [`Uta`], so the UI
//! never changes project data itself.
//!
//! They're synchronous, so Tauri runs them in the order they're sent, except
//! `set_buffer_size`, which reopens the device and mustn't hold up the main
//! thread.

use std::sync::{Mutex, MutexGuard, PoisonError};

use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use uta_core::{ClipId, Note, NoteId, SynthParam, TrackId};

use crate::menu::EditMenu;
use crate::uta::{Frame, ProjectView, Uta};

/// The event sent with a [`ProjectView`] after every change to the project,
/// wherever it came from (a command or the menu).
pub const PROJECT_CHANGED: &str = "project-changed";

/// The event sent when Copy, Paste or Duplicate is chosen from the Edit
/// menu, with the item's ID. The piano roll acts on it: the clipboard of
/// notes and the selection live in the UI, and what they change arrives as
/// ordinary commands.
pub const EDIT_MENU: &str = "edit-menu";

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

/// Sets one of a track's synth settings. Calls to the same setting with the
/// same `gesture` (one drag) undo as one step.
#[tauri::command]
pub fn set_synth_param<R: Runtime>(
    app: AppHandle<R>,
    track: TrackId,
    param: SynthParam,
    gesture: Option<u32>,
) -> Result<ProjectView, String> {
    edit(&app, |uta| uta.set_synth_param(track, param, gesture))
}

/// Adds notes to a clip. The UI picks each new note's ID. A `set_notes` of
/// the same notes with the same `gesture` undoes with it as one step.
#[tauri::command]
pub fn add_notes<R: Runtime>(
    app: AppHandle<R>,
    clip: ClipId,
    notes: Vec<Note>,
    gesture: Option<u32>,
) -> Result<ProjectView, String> {
    edit(&app, |uta| uta.add_notes(clip, notes, gesture))
}

/// Sets every value of existing notes in a clip. Calls with the same
/// `gesture` (one drag) undo as one step.
#[tauri::command]
pub fn set_notes<R: Runtime>(
    app: AppHandle<R>,
    clip: ClipId,
    notes: Vec<Note>,
    gesture: Option<u32>,
) -> Result<ProjectView, String> {
    edit(&app, |uta| uta.set_notes(clip, notes, gesture))
}

#[tauri::command]
pub fn remove_notes<R: Runtime>(
    app: AppHandle<R>,
    clip: ClipId,
    notes: Vec<NoteId>,
) -> Result<ProjectView, String> {
    edit(&app, |uta| uta.remove_notes(clip, notes))
}

/// Trims the notes of the same pitch that `notes` cover, as part of
/// `gesture`'s undo step: when a drag of them ends, or a paste lands.
#[tauri::command]
pub fn trim_notes<R: Runtime>(
    app: AppHandle<R>,
    clip: ClipId,
    notes: Vec<NoteId>,
    gesture: u32,
) -> Result<ProjectView, String> {
    edit(&app, |uta| uta.trim_notes(clip, notes, gesture))
}

/// Puts back everything `gesture` (one drag) changed: Esc mid-drag.
#[tauri::command]
pub fn cancel_gesture<R: Runtime>(app: AppHandle<R>, gesture: u32) -> Result<ProjectView, String> {
    edit(&app, |uta| {
        uta.cancel_gesture(gesture);
        Ok(())
    })
}

/// Plays a note briefly, without changing the project, even while stopped.
#[tauri::command]
pub fn audition_note(state: State<'_, AppState>, pitch: u8, velocity: u8) -> Result<(), String> {
    lock(&state.uta).audition(pitch, velocity)
}

/// Passes an Edit menu item (Copy, Paste or Duplicate) on to the UI.
pub fn edit_menu<R: Runtime>(app: &AppHandle<R>, item: &str) -> Result<(), String> {
    app.emit(EDIT_MENU, item).map_err(|error| error.to_string())
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
