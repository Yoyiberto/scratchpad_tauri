use std::fs;
use std::path::Path;

use tauri::{Manager, PhysicalPosition, PhysicalSize, RunEvent, WebviewWindow};

fn log(msg: &str) {
    use std::io::Write;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    println!("[{ts}] {msg}");
    let path = if cfg!(windows) {
        std::path::PathBuf::from("C:\\Tools\\scratchpad_events.txt")
    } else {
        std::env::temp_dir().join("scratchpad_events.txt")
    };
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "[{ts}] {msg}");
    }
}

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

    // Auto-migrate from previous plain-notepad directory if present (Windows & Linux)
    let mut potential_old_dirs = Vec::new();
    if let Ok(base) = std::env::var("LOCALAPPDATA") {
        potential_old_dirs.push(std::path::PathBuf::from(&base).join("plain-notepad"));
        potential_old_dirs.push(std::path::PathBuf::from(&base).join("com.ramse.plain-notepad"));
    }
    if let Ok(home) = std::env::var("HOME") {
        potential_old_dirs.push(
            std::path::PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("com.ramse.plain-notepad"),
        );
    }
    for old_dir in potential_old_dirs {
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

/// Ventana acoplada al lateral derecho del área de trabajo (sin barra de tareas)
/// del monitor actual. Todo en píxeles FÍSICOS para no mezclar escalas.
fn place_right_side(window: &WebviewWindow) -> tauri::Result<()> {
    let monitor = match window.current_monitor()? {
        Some(m) => Some(m),
        None => window.primary_monitor()?,
    };
    let Some(monitor) = monitor else {
        log("no monitor found -> center()");
        return window.center();
    };

    let scale = monitor.scale_factor();
    let wa = monitor.work_area();
    let (ax, ay) = (wa.position.x, wa.position.y);
    let (aw, ah) = (wa.size.width as i32, wa.size.height as i32);
    let margin = (12.0 * scale).round() as i32;

    let w = ((aw as f64 * 0.42).clamp(400.0 * scale, 560.0 * scale) as i32)
        .min(aw - 2 * margin)
        .max(1);
    let h = ((ah as f64 * 0.92).clamp(300.0 * scale, 980.0 * scale) as i32)
        .min(ah - 2 * margin)
        .max(1);
    let x = ax + aw - w - margin;
    let y = ay + (ah - h) / 2;

    log(&format!(
        "monitor {:?} scale={scale} work_area=({ax},{ay},{aw}x{ah}) -> win=({x},{y},{w}x{h})",
        monitor.name()
    ));

    window.set_size(PhysicalSize::new(w as u32, h as u32))?;
    window.set_position(PhysicalPosition::new(x, y))?;
    Ok(())
}

fn log_environment(app: &tauri::App) {
    log(&format!(
        "pid={} SESSIONNAME={:?} USERNAME={:?}",
        std::process::id(),
        std::env::var("SESSIONNAME").ok(),
        std::env::var("USERNAME").ok()
    ));
    if let Ok(mons) = app.available_monitors() {
        for m in mons {
            log(&format!(
                "monitor {:?} pos={:?} size={:?} scale={}",
                m.name(), m.position(), m.size(), m.scale_factor()
            ));
        }
    }
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
    let _ = std::fs::write("C:\\Tools\\scratchpad_win_close.txt", "win_close was called!\n");
    let _ = window.close();
}

#[tauri::command]
fn win_start_dragging(window: tauri::WebviewWindow) {
    let _ = window.start_dragging();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|window, event| {
            log(&format!("Window {:?} event: {:?}", window.label(), event));
        })
        .setup(|app| {
            log("setup running");
            log_environment(app);
            match app.get_webview_window("main") {
                Some(window) => {
                    if let Some(icon) = app.default_window_icon() {
                        let _ = window.set_icon(icon.clone());
                    }
                    if let Err(e) = place_right_side(&window) {
                        log(&format!("place_right_side error: {e}"));
                        let _ = window.center();
                    }
                    let _ = window.unminimize();
                    let _ = window.show();
                    let _ = window.set_focus();
                    log(&format!(
                        "after show: visible={:?} minimized={:?} pos={:?} size={:?}",
                        window.is_visible(),
                        window.is_minimized(),
                        window.outer_position(),
                        window.outer_size()
                    ));
                }
                None => log("main window NOT found"),
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
            win_close,
            win_start_dragging
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|_app, event| match event {
        RunEvent::ExitRequested { code, .. } => log(&format!("RunEvent::ExitRequested code={code:?}")),
        RunEvent::Exit => log("RunEvent::Exit"),
        _ => {}
    });
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
