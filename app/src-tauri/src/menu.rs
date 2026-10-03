//! The native menu bar. Undo, Redo, Copy, Paste and Duplicate are our own
//! items, not the system's, so ⌘Z, ⇧⌘Z, ⌘C, ⌘V and ⌘D reach the project
//! core, the timeline and the piano roll instead of the web view's text
//! editing. The Track menu adds, deletes and duplicates tracks.

use tauri::menu::{Menu, MenuBuilder, MenuItem, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Runtime};

use crate::uta::ProjectView;

pub const UNDO: &str = "undo";
pub const REDO: &str = "redo";
pub const COPY: &str = "copy";
pub const PASTE: &str = "paste";
pub const DUPLICATE: &str = "duplicate";
pub const ADD_TRACK: &str = "add-track";
pub const DELETE_TRACK: &str = "delete-track";
pub const DUPLICATE_TRACK: &str = "duplicate-track";
pub const ADD_STRESS_NOTES: &str = "add-stress-notes";

/// The menu items that are enabled or not by the project's state, kept to
/// update them as it changes.
pub struct MenuState<R: Runtime> {
    undo: MenuItem<R>,
    redo: MenuItem<R>,
    add_track: MenuItem<R>,
    duplicate_track: MenuItem<R>,
}

impl<R: Runtime> MenuState<R> {
    /// Enables Undo and Redo when there's something to undo or redo, and
    /// Add and Duplicate while there's room for another track.
    pub fn update(&self, project: &ProjectView) -> tauri::Result<()> {
        self.undo.set_enabled(project.can_undo)?;
        self.redo.set_enabled(project.can_redo)?;
        let room = project.tracks.len() < project.max_tracks;
        self.add_track.set_enabled(room)?;
        self.duplicate_track.set_enabled(room)
    }
}

/// Builds the menu bar: the usual app, Edit and Window menus, with our own
/// Undo, Redo, Copy, Paste and Duplicate, a Track menu, and a Develop menu
/// of tools for testing Uta itself.
pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<(Menu<R>, MenuState<R>)> {
    let undo = MenuItemBuilder::with_id(UNDO, "Undo")
        .accelerator("CmdOrCtrl+Z")
        .enabled(false)
        .build(app)?;
    let redo = MenuItemBuilder::with_id(REDO, "Redo")
        .accelerator("CmdOrCtrl+Shift+Z")
        .enabled(false)
        .build(app)?;
    let copy = MenuItemBuilder::with_id(COPY, "Copy")
        .accelerator("CmdOrCtrl+C")
        .build(app)?;
    let paste = MenuItemBuilder::with_id(PASTE, "Paste")
        .accelerator("CmdOrCtrl+V")
        .build(app)?;
    let duplicate = MenuItemBuilder::with_id(DUPLICATE, "Duplicate")
        .accelerator("CmdOrCtrl+D")
        .build(app)?;
    let add_track = MenuItemBuilder::with_id(ADD_TRACK, "Add Track")
        .accelerator("CmdOrCtrl+T")
        .build(app)?;
    let duplicate_track = MenuItemBuilder::with_id(DUPLICATE_TRACK, "Duplicate Track")
        .accelerator("CmdOrCtrl+Shift+D")
        .build(app)?;
    let delete_track = MenuItemBuilder::with_id(DELETE_TRACK, "Delete Track")
        .accelerator("CmdOrCtrl+Backspace")
        .build(app)?;

    let app_menu = SubmenuBuilder::new(app, "Uta")
        .about(None)
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .quit()
        .build()?;
    let edit = SubmenuBuilder::new(app, "Edit")
        .item(&undo)
        .item(&redo)
        .separator()
        .cut()
        .item(&copy)
        .item(&paste)
        .item(&duplicate)
        .select_all()
        .build()?;
    let track = SubmenuBuilder::new(app, "Track")
        .item(&add_track)
        .item(&duplicate_track)
        .item(&delete_track)
        .build()?;
    let develop = SubmenuBuilder::new(app, "Develop")
        .item(&MenuItemBuilder::with_id(ADD_STRESS_NOTES, "Add Stress Notes").build(app)?)
        .build()?;
    let window = SubmenuBuilder::new(app, "Window")
        .minimize()
        .maximize()
        .separator()
        .close_window()
        .build()?;
    let menu = MenuBuilder::new(app)
        .items(&[&app_menu, &edit, &track, &develop, &window])
        .build()?;
    Ok((
        menu,
        MenuState {
            undo,
            redo,
            add_track,
            duplicate_track,
        },
    ))
}
