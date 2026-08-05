use std::fs;
use std::path::Path;

use tauri::{LogicalPosition, LogicalSize, Manager, Position, Size};

#[tauri::command]
fn read_text(path: String) -> Result<String, String> {
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn write_text(path: String, contents: String) -> Result<(), String> {
    if let Some(parent) = Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    fs::write(&path, contents).map_err(|e| e.to_string())
}

fn scan_snap_n(dir: &str, base: &str) -> Result<u32, String> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(0),
    };

    let prefix = format!("{base}_v");
    let mut max = 0u32;

    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with(&prefix) {
            continue;
        }
        let rest = &name[prefix.len()..];
        let num = match rest.find('.') {
            Some(i) => &rest[..i],
            None => rest,
        };
        if let Ok(n) = num.parse::<u32>() {
            max = max.max(n);
        }
    }
    Ok(max)
}

/// Highest existing `{base}_vN.*` in dir (0 if none).
#[tauri::command]
fn latest_snap_n(dir: String, base: String) -> Result<u32, String> {
    scan_snap_n(&dir, &base)
}

/// Next version number for `{base}_vN.*`.
#[tauri::command]
fn next_snap_n(dir: String, base: String) -> Result<u32, String> {
    Ok(scan_snap_n(&dir, &base)? + 1)
}

/// Local app data: `%LOCALAPPDATA%\plain-notepad` (no admin, no OneDrive sync).
#[tauri::command]
fn app_data_dir() -> Result<String, String> {
    let base = std::env::var("LOCALAPPDATA").map_err(|e| e.to_string())?;
    let dir = Path::new(&base).join("plain-notepad");
    fs::create_dir_all(dir.join("notes")).map_err(|e| e.to_string())?;
    Ok(dir.to_string_lossy().into_owned())
}

/// Thin window docked to the right side of the current monitor.
fn place_right_half(window: &tauri::WebviewWindow) {
    let Ok(Some(monitor)) = window.current_monitor() else {
        return;
    };
    let scale = monitor.scale_factor();
    let msize = monitor.size();
    let mpos = monitor.position();
    let sw = msize.width as f64 / scale;
    let sh = msize.height as f64 / scale;
    let mx = mpos.x as f64 / scale;
    let my = mpos.y as f64 / scale;

    // ~42% of screen width, capped so it stays clearly under half
    let w = (sw * 0.42).clamp(400.0, 560.0);
    let h = (sh * 0.88).clamp(520.0, 980.0);
    let x = mx + sw - w - 12.0;
    let y = my + ((sh - h) * 0.5).max(20.0);

    let _ = window.set_size(Size::Logical(LogicalSize {
        width: w,
        height: h,
    }));
    let _ = window.set_position(Position::Logical(LogicalPosition { x, y }));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                place_right_half(&window);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            read_text,
            write_text,
            latest_snap_n,
            next_snap_n,
            app_data_dir
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
