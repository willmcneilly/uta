//! Where the window opens. The first time, it fills the main screen
//! (maximised, not full screen). After that, Tauri's window-state plugin
//! opens it at the size and position it was closed at, unless that place is
//! on no screen now, say because a display was unplugged: then it fills the
//! main screen again. A window closed maximised is saved with no size of its
//! own until it's unmaximised, and that fills the main screen too.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;
use tauri::{App, Manager, PhysicalPosition, Runtime, WebviewWindow};
use tauri_plugin_window_state::{AppHandleExt, StateFlags, WindowExt};

/// The window's label in `tauri.conf.json`.
pub const MAIN: &str = "main";

/// What the plugin saves and restores: size, position and whether it was
/// maximised. Not visibility, because the window is shown here once it's
/// placed, so it never flashes at its default size first.
pub fn state_flags() -> StateFlags {
    StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED
}

/// A rectangle on the desktop, in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    fn overlaps(&self, other: &Rect) -> bool {
        let right = |r: &Rect| i64::from(r.x) + i64::from(r.width);
        let bottom = |r: &Rect| i64::from(r.y) + i64::from(r.height);
        i64::from(self.x) < right(other)
            && i64::from(other.x) < right(self)
            && i64::from(self.y) < bottom(other)
            && i64::from(other.y) < bottom(self)
    }
}

/// How the window opens.
#[derive(Debug, PartialEq)]
pub enum Opening {
    /// Where it was closed, restored by the plugin.
    Restored,
    /// Maximised on the main screen.
    FillMainScreen,
}

/// How the window opens, from where it was saved (if anywhere) and the
/// screens there are now.
pub fn opening(saved: Option<Rect>, screens: &[Rect]) -> Opening {
    match saved {
        // A rectangle with no size overlaps nothing, so it fills too.
        Some(rect) if screens.iter().any(|screen| rect.overlaps(screen)) => Opening::Restored,
        _ => Opening::FillMainScreen,
    }
}

/// The part of one window's entry in the plugin's file that we read.
#[derive(Deserialize)]
struct Saved {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

/// Where the window `label` was saved in the plugin's file at `path`, or
/// None if it never was or the file can't be read.
fn saved(path: &Path, label: &str) -> Option<Rect> {
    let file = std::fs::read(path).ok()?;
    let mut windows: HashMap<String, Saved> = serde_json::from_slice(&file).ok()?;
    let Saved {
        x,
        y,
        width,
        height,
    } = windows.remove(label)?;
    Some(Rect {
        x,
        y,
        width,
        height,
    })
}

fn monitor_rect(monitor: &tauri::Monitor) -> Rect {
    Rect {
        x: monitor.position().x,
        y: monitor.position().y,
        width: monitor.size().width,
        height: monitor.size().height,
    }
}

/// Places the main window and shows it: where the plugin saved it, or
/// filling the main screen. The plugin is built with `skip_initial_state`,
/// so it restores only when this says so, and a window that's going to fill
/// the screen is never first maximised somewhere else. Placing is best
/// effort: if it fails, the window is still shown, wherever it is.
pub fn place<R: Runtime>(app: &App<R>) -> tauri::Result<()> {
    let Some(window) = app.get_webview_window(MAIN) else {
        return Ok(());
    };
    if let Err(error) = restore_or_fill(app, &window) {
        eprintln!("uta: couldn't place the window: {error}");
    }
    window.show()?;
    window.set_focus()
}

fn restore_or_fill<R: Runtime>(app: &App<R>, window: &WebviewWindow<R>) -> tauri::Result<()> {
    let path = app.path().app_config_dir()?.join(app.handle().filename());
    let screens: Vec<Rect> = window
        .available_monitors()?
        .iter()
        .map(monitor_rect)
        .collect();
    match opening(saved(&path, MAIN), &screens) {
        Opening::Restored => window.restore_state(state_flags()),
        Opening::FillMainScreen => {
            // Maximising fills the screen the window is on, so move it to
            // the main one first.
            if let Some(main) = window.primary_monitor()? {
                let PhysicalPosition { x, y } = *main.position();
                window.set_position(PhysicalPosition { x, y })?;
            }
            window.maximize()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAPTOP: Rect = Rect {
        x: 0,
        y: 0,
        width: 3024,
        height: 1964,
    };
    /// A display to the laptop's right.
    const DISPLAY: Rect = Rect {
        x: 3024,
        y: 0,
        width: 5120,
        height: 2880,
    };

    fn window(x: i32, y: i32) -> Rect {
        Rect {
            x,
            y,
            width: 2048,
            height: 1400,
        }
    }

    #[test]
    fn the_first_launch_fills_the_main_screen() {
        assert_eq!(opening(None, &[LAPTOP]), Opening::FillMainScreen);
    }

    #[test]
    fn a_window_saved_on_a_screen_opens_where_it_was() {
        assert_eq!(opening(Some(window(100, 80)), &[LAPTOP]), Opening::Restored);
        assert_eq!(
            opening(Some(window(4000, 200)), &[LAPTOP, DISPLAY]),
            Opening::Restored
        );
    }

    #[test]
    fn a_window_partly_on_a_screen_opens_where_it_was() {
        // Mostly off the laptop's left edge, but its right side still shows.
        assert_eq!(
            opening(Some(window(-1900, 100)), &[LAPTOP]),
            Opening::Restored
        );
    }

    #[test]
    fn a_window_on_an_unplugged_display_fills_the_main_screen() {
        assert_eq!(
            opening(Some(window(4000, 200)), &[LAPTOP]),
            Opening::FillMainScreen
        );
        // Just touching the laptop's edge isn't on it.
        assert_eq!(
            opening(Some(window(3024, 0)), &[LAPTOP]),
            Opening::FillMainScreen
        );
    }

    #[test]
    fn a_window_saved_with_no_size_fills_the_main_screen() {
        let empty = Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
        assert_eq!(opening(Some(empty), &[LAPTOP]), Opening::FillMainScreen);
    }

    #[test]
    fn reads_the_window_from_the_plugins_file() {
        let dir = std::env::temp_dir().join(format!("uta-window-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".window-state.json");
        // As the plugin writes it.
        std::fs::write(
            &path,
            r#"{"main":{"width":2048,"height":1400,"x":120,"y":64,"prev_x":0,"prev_y":0,
                "maximized":false,"visible":true,"decorated":true,"fullscreen":false}}"#,
        )
        .unwrap();
        assert_eq!(saved(&path, MAIN), Some(window(120, 64)));
        assert_eq!(saved(&path, "other"), None);

        std::fs::write(&path, "not json").unwrap();
        assert_eq!(saved(&path, MAIN), None);
        assert_eq!(saved(&dir.join("missing.json"), MAIN), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
