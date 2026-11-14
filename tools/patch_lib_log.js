import fs from "node:fs";

let c = fs.readFileSync("src-tauri/src/lib.rs", "utf8");
const isCRLF = c.includes("\r\n");
let norm = c.replace(/\r\n/g, "\n");

const target = `        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                place_right_half(&window);
                if let Some(icon) = app.default_window_icon() {
                    let _ = window.set_icon(icon.clone());
                }
            }
            Ok(())
        })`;

const repl = `        .setup(|app| {
            let _ = std::fs::write("C:\\\\Tools\\\\scratchpad_setup.txt", "setup running\\n");
            if let Some(window) = app.get_webview_window("main") {
                let _ = std::fs::write("C:\\\\Tools\\\\scratchpad_setup.txt", "main window found\\n");
                place_right_half(&window);
                if let Some(icon) = app.default_window_icon() {
                    let _ = window.set_icon(icon.clone());
                }
                let _ = window.show();
            } else {
                let _ = std::fs::write("C:\\\\Tools\\\\scratchpad_setup.txt", "main window NOT found\\n");
            }
            Ok(())
        })`;

if (!norm.includes(target)) {
  console.error("target not found in lib.rs");
  process.exit(1);
}

norm = norm.replace(target, repl);
if (isCRLF) norm = norm.replace(/\n/g, "\r\n");

fs.writeFileSync("src-tauri/src/lib.rs", norm, "utf8");
console.log("src-tauri/src/lib.rs patched with diagnostic logging");
