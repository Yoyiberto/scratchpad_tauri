// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "windows")]
extern "system" {
    fn AttachConsole(dw_process_id: u32) -> i32;
}

fn main() {
    #[cfg(target_os = "windows")]
    unsafe {
        // Conecta la consola si se ejecuta desde PowerShell / CMD / Windows Terminal
        AttachConsole(0xFFFFFFFF);
    }

    std::panic::set_hook(Box::new(|info| {
        let msg = format!("PANIC: {:?}\n", info);
        eprintln!("{msg}");
        let _ = std::fs::write("C:\\Tools\\scratchpad_crash.txt", msg);
    }));

    let _ = std::fs::write("C:\\Tools\\scratchpad_started.txt", "main started\n");
    println!("=== Scratchpad iniciado ===");
    scratchpad_lib::run();
}
