//! The Tauri commands the UI calls. Each one goes through [`Uta`], so the UI
//! never changes project data itself.
//!
//! They're synchronous, so Tauri runs them in the order they're sent, except
//! `set_buffer_size`, which reopens the device and mustn't hold up the main
//! thread.

use std::sync::{Mutex, MutexGuard, PoisonError};

use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use uta_core::time::Ticks;
use uta_core::{ClipId, ClipPosition, Note, NoteId, SynthParam, TrackId};

use crate::menu::MenuState;
use crate::uta::{Frame, MixerView, PastedClip, ProjectView, Uta};

/// The event sent with a [`ProjectView`] after every change to the project,
/// wherever it came from (a command or the menu).
pub const PROJECT_CHANGED: &str = "project-changed";

/// The event sent when Copy, Paste or Duplicate is chosen from the Edit
/// menu, with the item's ID. The timeline or the piano roll acts on it,
/// whichever was last clicked in: the clipboards and the selections live in
/// the UI, and what they change arrives as ordinary commands.
pub const EDIT_MENU: &str = "edit-menu";

/// The event sent when an item is chosen from the Track menu (Add, Delete
/// or Duplicate), with the item's ID. The UI acts on it, because which track
/// is selected is the UI's: it sends back an ordinary command.
pub const TRACK_MENU: &str = "track-menu";

/// The event sent when an item is chosen from the Develop menu, with the
/// item's ID. The UI sends it back with the clip it's showing.
pub const DEVELOP_MENU: &str = "develop-menu";

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
    if let Some(menu) = app.try_state::<MenuState<R>>() {
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

/// Sets the loop region, in whole bars. Calls with the same `gesture` (one
/// drag on the ruler) undo as one step.
#[tauri::command]
pub fn set_loop<R: Runtime>(
    app: AppHandle<R>,
    start_bar: u32,
    bars: u32,
    gesture: Option<u32>,
) -> Result<ProjectView, String> {
    edit(&app, |uta| uta.set_loop(start_bar, bars, gesture))
}

/// Switches the loop on or off.
#[tauri::command]
pub fn set_loop_enabled<R: Runtime>(
    app: AppHandle<R>,
    enabled: bool,
) -> Result<ProjectView, String> {
    edit(&app, |uta| uta.set_loop_enabled(enabled))
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

/// Sets a track's volume, pan, mute and solo. Calls to the same track with
/// the same `gesture` (one drag) undo as one step.
#[tauri::command]
pub fn set_track_mixer<R: Runtime>(
    app: AppHandle<R>,
    track: TrackId,
    mixer: MixerView,
    gesture: Option<u32>,
) -> Result<ProjectView, String> {
    edit(&app, |uta| {
        uta.set_track_mixer(track, mixer.into(), gesture)
    })
}

/// Solos a track on its own, or unsolos it if it already is: ⌥-click on
/// Solo. One undo step.
#[tauri::command]
pub fn solo_track_alone<R: Runtime>(
    app: AppHandle<R>,
    track: TrackId,
) -> Result<ProjectView, String> {
    edit(&app, |uta| uta.solo_alone(track))
}

/// Adds a synth track below the others. The UI picks its ID.
#[tauri::command]
pub fn add_track<R: Runtime>(app: AppHandle<R>, id: TrackId) -> Result<ProjectView, String> {
    edit(&app, |uta| uta.add_track(id))
}

/// Adds a copy of a track, with its sound, mixer and clips, below it. The UI
/// picks the copy's ID; Rust picks its clips' and notes'.
#[tauri::command]
pub fn duplicate_track<R: Runtime>(
    app: AppHandle<R>,
    track: TrackId,
    id: TrackId,
) -> Result<ProjectView, String> {
    edit(&app, |uta| uta.duplicate_track(track, id))
}

#[tauri::command]
pub fn remove_track<R: Runtime>(app: AppHandle<R>, track: TrackId) -> Result<ProjectView, String> {
    edit(&app, |uta| uta.remove_track(track))
}

/// Moves a track to `index` in the order, counting from 0 at the top.
#[tauri::command]
pub fn move_track<R: Runtime>(
    app: AppHandle<R>,
    track: TrackId,
    index: usize,
) -> Result<ProjectView, String> {
    edit(&app, |uta| uta.move_track(track, index))
}

/// Adds an empty clip to a track: drawn on the timeline. The UI picks its
/// ID.
#[tauri::command]
pub fn add_clip<R: Runtime>(
    app: AppHandle<R>,
    track: TrackId,
    id: ClipId,
    start: Ticks,
    length: Ticks,
) -> Result<ProjectView, String> {
    edit(&app, |uta| uta.add_clip(track, id, start, length))
}

/// Sets clips' track, start and length. Calls with the same `gesture` (one
/// drag) undo as one step.
#[tauri::command]
pub fn set_clips<R: Runtime>(
    app: AppHandle<R>,
    clips: Vec<ClipPosition>,
    gesture: Option<u32>,
) -> Result<ProjectView, String> {
    edit(&app, |uta| uta.set_clips(clips, gesture))
}

/// Adds copies of clips: a paste or a duplicate. The UI picks each copy's
/// ID; Rust picks its notes'. One undo step.
#[tauri::command]
pub fn paste_clips<R: Runtime>(
    app: AppHandle<R>,
    clips: Vec<PastedClip>,
) -> Result<ProjectView, String> {
    edit(&app, |uta| uta.paste_clips(clips))
}

#[tauri::command]
pub fn remove_clips<R: Runtime>(
    app: AppHandle<R>,
    clips: Vec<ClipId>,
) -> Result<ProjectView, String> {
    edit(&app, |uta| uta.remove_clips(clips))
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

/// Plays a note briefly on a track, without changing the project, even
/// while stopped.
#[tauri::command]
pub fn audition_note(
    state: State<'_, AppState>,
    track: TrackId,
    pitch: u8,
    velocity: u8,
) -> Result<(), String> {
    lock(&state.uta).audition(track, pitch, velocity)
}

/// Passes a menu item on to the UI as `event`: Copy, Paste and Duplicate
/// from the Edit menu, the Track menu's items, and the Develop menu's.
pub fn pass_menu_item<R: Runtime>(
    app: &AppHandle<R>,
    event: &str,
    item: &str,
) -> Result<(), String> {
    app.emit(event, item).map_err(|error| error.to_string())
}

/// Fills a clip with a few thousand notes, as one undo step. The UI calls it
/// from the Develop menu, with the clip it's showing.
#[tauri::command]
pub fn add_stress_notes<R: Runtime>(
    app: AppHandle<R>,
    clip: ClipId,
) -> Result<ProjectView, String> {
    edit(&app, |uta| uta.add_stress_notes(clip))
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
pub fn pause(state: State<'_, AppState>) -> Result<(), String> {
    lock(&state.uta).pause()
}

#[tauri::command]
pub fn resume(state: State<'_, AppState>) -> Result<(), String> {
    lock(&state.uta).resume()
}

/// Moves the play start while stopped, or jumps there while playing.
#[tauri::command]
pub fn locate(state: State<'_, AppState>, ticks: Ticks) -> Result<(), String> {
    lock(&state.uta).locate(ticks)
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
