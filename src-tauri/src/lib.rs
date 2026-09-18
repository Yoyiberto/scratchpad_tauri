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

/// Local app data: `%LOCALAPPDATA%\scratchpad` on Windows, or standard app local data dir on Linux/macOS.
#[tauri::command]
fn app_data_dir(app: tauri::AppHandle) -> Result<String, String> {
    let dir = if let Ok(base) = std::env::var("LOCALAPPDATA") {
        std::path::PathBuf::from(base).join("scratchpad")
    } else if let Ok(path) = app.path().app_local_data_dir() {
        path
    } else if let Ok(home) = std::env::var("HOME") {
        std::path::PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("scratchpad")
    } else {
        std::path::PathBuf::from(".scratchpad")
    };
    fs::create_dir_all(dir.join("notes")).map_err(|e| e.to_string())?;

    // Auto-migrate from previous plain-notepad directory if present
    if let Ok(home) = std::env::var("HOME") {
        let old_dir = std::path::PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("com.ramse.plain-notepad");
        if old_dir.exists() {
            let old_session = old_dir.join("session.json");
            let new_session = dir.join("session.json");
            if old_session.exists() && !new_session.exists() {
                let _ = fs::copy(&old_session, &new_session);
            }
            let old_corr = old_dir.join("corrections.json");
            let new_corr = dir.join("corrections.json");
            if old_corr.exists() && !new_corr.exists() {
                let _ = fs::copy(&old_corr, &new_corr);
            }
        }
    }

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

fn get_corrections_file_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let base_dir = app_data_dir(app.clone())?;
    Ok(Path::new(&base_dir).join("corrections.json"))
}

#[tauri::command]
fn save_correction(app: tauri::AppHandle, correction: serde_json::Value) -> Result<usize, String> {
    let corr_path = get_corrections_file_path(&app)?;
    let mut list: Vec<serde_json::Value> = if corr_path.exists() {
        let content = fs::read_to_string(&corr_path).unwrap_or_else(|_| "[]".into());
        serde_json::from_str(&content).unwrap_or_else(|_| Vec::new())
    } else {
        Vec::new()
    };
    list.push(correction);
    let serialized = serde_json::to_string_pretty(&list).map_err(|e| e.to_string())?;
    fs::write(&corr_path, serialized).map_err(|e| e.to_string())?;
    Ok(list.len())
}

#[tauri::command]
fn load_corrections(app: tauri::AppHandle) -> Result<Vec<serde_json::Value>, String> {
    let corr_path = get_corrections_file_path(&app)?;
    if !corr_path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&corr_path).map_err(|e| e.to_string())?;
    let list: Vec<serde_json::Value> = serde_json::from_str(&content).unwrap_or_else(|_| Vec::new());
    Ok(list)
}

#[tauri::command]
fn clear_corrections(app: tauri::AppHandle) -> Result<(), String> {
    let corr_path = get_corrections_file_path(&app)?;
    if corr_path.exists() {
        fs::write(&corr_path, "[]").map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn get_test_screen() -> Option<String> {
    std::env::var("NOTEPAD_TEST_SCREEN").ok()
}

#[tauri::command]
fn win_minimize(window: tauri::WebviewWindow) {
    let _ = window.minimize();
}

#[tauri::command]
fn win_toggle_maximize(window: tauri::WebviewWindow) {
    if let Ok(is_max) = window.is_maximized() {
        if is_max {
            let _ = window.unmaximize();
        } else {
            let _ = window.maximize();
        }
    }
}

#[tauri::command]
fn win_close(window: tauri::WebviewWindow) {
    let _ = window.close();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                place_right_half(&window);
                if let Some(icon) = app.default_window_icon() {
                    let _ = window.set_icon(icon.clone());
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            read_text,
            write_text,
            latest_snap_n,
            next_snap_n,
            app_data_dir,
            save_correction,
            load_corrections,
            clear_corrections,
            get_test_screen,
            win_minimize,
            win_toggle_maximize,
            win_close
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_write_and_read_text() {
        let temp_dir = std::env::temp_dir().join("scratchpad_test_rw");
        let _ = fs::remove_dir_all(&temp_dir);
        let test_file = temp_dir.join("sub").join("test.txt");
        let test_path = test_file.to_string_lossy().to_string();

        let write_res = write_text(test_path.clone(), "Hello Linux Tauri!".into());
        assert!(write_res.is_ok());

        let read_res = read_text(test_path);
        assert_eq!(read_res.unwrap(), "Hello Linux Tauri!");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_scan_snap_n_empty_and_nonexistent() {
        assert_eq!(scan_snap_n("/nonexistent_path_123456", "note").unwrap(), 0);

        let temp_dir = std::env::temp_dir().join("scratchpad_test_snap_empty");
        fs::create_dir_all(&temp_dir).unwrap();
        assert_eq!(scan_snap_n(&temp_dir.to_string_lossy(), "note").unwrap(), 0);
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_scan_snap_n_versions() {
        let temp_dir = std::env::temp_dir().join("scratchpad_test_snap_versions");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        fs::write(temp_dir.join("note_v1.txt"), "v1").unwrap();
        fs::write(temp_dir.join("note_v2.txt"), "v2").unwrap();
        fs::write(temp_dir.join("note_v10.txt"), "v10").unwrap();
        fs::write(temp_dir.join("note_v5.md"), "v5").unwrap();
        fs::write(temp_dir.join("other_v99.txt"), "other").unwrap();
        fs::write(temp_dir.join("note_vbad.txt"), "bad").unwrap();

        let dir_str = temp_dir.to_string_lossy().to_string();
        assert_eq!(latest_snap_n(dir_str.clone(), "note".into()).unwrap(), 10);
        assert_eq!(next_snap_n(dir_str.clone(), "note".into()).unwrap(), 11);
        assert_eq!(latest_snap_n(dir_str.clone(), "other".into()).unwrap(), 99);
        assert_eq!(next_snap_n(dir_str.clone(), "other".into()).unwrap(), 100);
        assert_eq!(latest_snap_n(dir_str.clone(), "unmatched".into()).unwrap(), 0);
        assert_eq!(next_snap_n(dir_str.clone(), "unmatched".into()).unwrap(), 1);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_correction_serialization() {
        let temp_dir = std::env::temp_dir().join("scratchpad_test_corr");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        let file_path = temp_dir.join("corrections.json");

        let item = serde_json::json!({
            "original": "quitar",
            "corrected": "source",
            "context": "-improve abstract\n-quitar source..",
            "timestamp": "2026-09-14T12:00:00Z"
        });

        let list = vec![item.clone()];
        let json_data = serde_json::to_string_pretty(&list).unwrap();
        fs::write(&file_path, json_data).unwrap();

        let read_back = fs::read_to_string(&file_path).unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&read_back).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["original"], "quitar");
        assert_eq!(parsed[0]["corrected"], "source");

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
