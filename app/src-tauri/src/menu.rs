//! The native menu bar. Undo and Redo are our own items, not the system's,
//! so ⌘Z and ⇧⌘Z reach the project core instead of the web view's text
//! editing.

use tauri::menu::{Menu, MenuBuilder, MenuItem, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Runtime};

use crate::uta::ProjectView;

pub const UNDO: &str = "undo";
pub const REDO: &str = "redo";

/// The Edit menu's Undo and Redo, kept to enable them as the history changes.
pub struct EditMenu<R: Runtime> {
    undo: MenuItem<R>,
    redo: MenuItem<R>,
}

impl<R: Runtime> EditMenu<R> {
    /// Enables Undo and Redo when there's something to undo or redo.
    pub fn update(&self, project: &ProjectView) -> tauri::Result<()> {
        self.undo.set_enabled(project.can_undo)?;
        self.redo.set_enabled(project.can_redo)
    }
}

/// Builds the menu bar: the usual app, Edit and Window menus, with our own
/// Undo and Redo.
pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<(Menu<R>, EditMenu<R>)> {
    let undo = MenuItemBuilder::with_id(UNDO, "Undo")
        .accelerator("CmdOrCtrl+Z")
        .enabled(false)
        .build(app)?;
    let redo = MenuItemBuilder::with_id(REDO, "Redo")
        .accelerator("CmdOrCtrl+Shift+Z")
        .enabled(false)
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
        .copy()
        .paste()
        .select_all()
        .build()?;
    let window = SubmenuBuilder::new(app, "Window")
        .minimize()
        .maximize()
        .separator()
        .close_window()
        .build()?;
    let menu = MenuBuilder::new(app)
        .items(&[&app_menu, &edit, &window])
        .build()?;
    Ok((menu, EditMenu { undo, redo }))
}
