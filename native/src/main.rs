#![cfg_attr(not(test), windows_subsystem = "windows")]
//! Scratchpad en egui: pestañas, sesión autoguardada y un historial
//! automático "escrito → limpio" que sirve como dataset de cómo limpias.

mod store;
mod tracker;

use eframe::egui::{
    self, text::LayoutJob, Align, Align2, Color32, CursorIcon, FontData,
    FontDefinitions, FontFamily, FontId, Id, Key, KeyboardShortcut, Layout, Margin, Modifiers,
    Rect, ResizeDirection, Rounding, RichText, ScrollArea, Sense, Stroke, TextEdit, Theme, ThemePreference,
    Vec2, ViewportCommand,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};
use store::{fnv, now_ts, Store, Version};
use tracker::{Checkpoint, Phase, Tracker};

const FONT_MIN: f32 = 10.0;
const FONT_MAX: f32 = 48.0;
const FONT_DEFAULT: f32 = 14.0;
const BAR_H: f32 = 30.0;
const GC_IDLE: Duration = Duration::from_secs(7 * 24 * 3600);

// ───────────────────────────── persistencia ─────────────────────────────

fn data_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("scratchpad")
}

fn write_atomic(path: &Path, contents: &str) {
    let tmp = path.with_extension("tmp");
    if fs::write(&tmp, contents).is_ok() {
        let _ = fs::rename(&tmp, path);
    }
}

fn yes() -> bool {
    true
}

/// Mismo formato que session.json de la versión Tauri (se puede volver atrás).
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct TabData {
    id: u64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    content: String,
    #[serde(default)]
    saved: String,
    #[serde(default)]
    thread_base: Option<String>,
    #[serde(default)]
    snap_n: u32,
    #[serde(default = "yes")]
    wrap: bool,
    #[serde(default)]
    note_id: String,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Session {
    #[serde(default)]
    active_id: u64,
    #[serde(default)]
    next_id: u64,
    #[serde(default)]
    tabs: Vec<TabData>,
}

#[derive(Serialize, Deserialize)]
struct Settings {
    #[serde(default = "theme_default")]
    theme: String,
    #[serde(default = "font_default")]
    font_size: f32,
    #[serde(default)]
    last_gc: String,
}

fn theme_default() -> String {
    "system".into()
}
fn font_default() -> f32 {
    FONT_DEFAULT
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: theme_default(),
            font_size: FONT_DEFAULT,
            last_gc: String::new(),
        }
    }
}

fn sanitize(s: &str) -> String {
    let out: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .take(40)
        .collect();
    if out.is_empty() {
        "nota".into()
    } else {
        out
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn file_note_id(path: &str) -> String {
    let name = file_name(path);
    let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
    format!("f-{}-{:08x}", sanitize(stem), fnv(&path.to_lowercase()) as u32)
}

fn new_note_id(id: u64) -> String {
    format!("n{}-{id}", now_ts())
}

// ───────────────────────────── estado ─────────────────────────────

struct Tab {
    d: TabData,
    /// Texto en el frame anterior (para medir cada edición).
    prev: String,
    tr: Tracker,
    cursor: usize,
}

impl Tab {
    fn new(d: TabData) -> Self {
        Self {
            prev: d.content.clone(),
            tr: Tracker::new(&d.content),
            cursor: 0,
            d,
        }
    }

    fn title(&self) -> String {
        if let Some(p) = &self.d.path {
            return file_name(p).to_owned();
        }
        let first = self.d.content.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
        if first.is_empty() {
            return "Nota nueva".into();
        }
        let mut t: String = first.chars().take(22).collect();
        if first.chars().count() > 22 {
            t.push('…');
        }
        t
    }

    fn file_dirty(&self) -> bool {
        self.d.path.is_some() && self.d.content != self.d.saved
    }
}

struct Correction {
    start: usize,
    end: usize,
    word: String,
    paragraph: String,
    new: String,
    focus: bool,
}

struct App {
    dir: PathBuf,
    notes_dir: PathBuf,
    store: Store,
    settings: Settings,
    tabs: Vec<Tab>,
    active: usize,
    next_id: u64,
    session_dirty: Option<Instant>,
    focus_editor: bool,
    history_open: bool,
    history: Vec<Version>,
    history_for: String,
    preview: Option<(usize, String)>,
    counts: HashMap<String, usize>,
    status: Option<(String, Instant)>,
    correction: Option<Correction>,
    stats_cache: Option<(Instant, String)>,
    last_gc_check: Instant,
    setup_done: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let dir = data_dir();
        let notes_dir = dir.join("notes");
        let _ = fs::create_dir_all(&notes_dir);
        let mut store = Store::new(dir.join("history"));
        store.import_tauri_snaps(&notes_dir);

        let settings: Settings = fs::read_to_string(dir.join("settings.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        let session: Session = fs::read_to_string(dir.join("session.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        let mut next_id = session.next_id.max(1);
        let mut tabs: Vec<Tab> = session
            .tabs
            .into_iter()
            .map(|mut d| {
                if d.note_id.is_empty() {
                    d.note_id = match &d.path {
                        Some(p) => file_note_id(p),
                        None => new_note_id(d.id),
                    };
                }
                next_id = next_id.max(d.id + 1);
                Tab::new(d)
            })
            .collect();
        if tabs.is_empty() {
            tabs.push(Tab::new(TabData {
                id: next_id,
                wrap: true,
                note_id: new_note_id(next_id),
                ..Default::default()
            }));
            next_id += 1;
        }
        let active = tabs.iter().position(|t| t.d.id == session.active_id).unwrap_or(0);

        setup_fonts(&cc.egui_ctx);
        setup_style(&cc.egui_ctx);
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);

        let mut app = Self {
            dir,
            notes_dir,
            store,
            settings,
            tabs,
            active,
            next_id,
            session_dirty: None,
            focus_editor: true,
            history_open: false,
            history: Vec::new(),
            history_for: String::new(),
            preview: None,
            counts: HashMap::new(),
            status: None,
            correction: None,
            stats_cache: None,
            last_gc_check: Instant::now() - Duration::from_secs(7200),
            setup_done: false,
        };
        app.apply_theme(&cc.egui_ctx);
        app
    }

    fn tab(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    fn set_status(&mut self, s: impl Into<String>) {
        self.status = Some((s.into(), Instant::now()));
    }

    fn mark_dirty(&mut self) {
        if self.session_dirty.is_none() {
            self.session_dirty = Some(Instant::now());
        }
    }

    fn save_session(&mut self) {
        let s = Session {
            active_id: self.tabs[self.active].d.id,
            next_id: self.next_id,
            tabs: self.tabs.iter().map(|t| t.d.clone()).collect(),
        };
        if let Ok(j) = serde_json::to_string_pretty(&s) {
            write_atomic(&self.dir.join("session.json"), &j);
        }
        self.session_dirty = None;
    }

    fn save_settings(&self) {
        if let Ok(j) = serde_json::to_string_pretty(&self.settings) {
            write_atomic(&self.dir.join("settings.json"), &j);
        }
    }

    fn apply_theme(&self, ctx: &egui::Context) {
        ctx.set_theme(match self.settings.theme.as_str() {
            "light" => ThemePreference::Light,
            "dark" => ThemePreference::Dark,
            _ => ThemePreference::System,
        });
    }

    fn set_font(&mut self, size: f32) {
        self.settings.font_size = size.clamp(FONT_MIN, FONT_MAX).round();
        self.save_settings();
        self.set_status(format!("Fuente {} px", self.settings.font_size));
    }

    // ── historial ──

    fn save_version(&mut self, i: usize, kind: &str, text: &str) -> bool {
        let note = self.tabs[i].d.note_id.clone();
        let saved = self.store.save(&note, kind, text, &now_ts()).is_some();
        if saved {
            self.counts.remove(&note);
            if self.history_for == note {
                self.history_for.clear(); // fuerza recarga del panel
            }
        }
        saved
    }

    fn checkpoint(&mut self, i: usize, cp: Checkpoint) {
        let (kind, text) = match cp {
            Checkpoint::Escrito(t) => ("escrito", t),
            Checkpoint::Limpio(t) => ("limpio", t),
        };
        if self.save_version(i, kind, &text) {
            let kind = if store::is_discard(&text) { "descartado" } else { kind };
            self.set_status(format!("Versión «{kind}» guardada en el historial"));
        }
    }

    fn flush_tab(&mut self, i: usize) {
        let content = self.tabs[i].d.content.clone();
        if let Some(cp) = self.tabs[i].tr.flush(&content) {
            self.checkpoint(i, cp);
        }
    }

    fn version_count(&mut self, note: &str) -> usize {
        if let Some(n) = self.counts.get(note) {
            return *n;
        }
        let n = self.store.list(note).len();
        self.counts.insert(note.to_owned(), n);
        n
    }

    fn snap(&mut self) {
        let i = self.active;
        let content = self.tabs[i].d.content.clone();
        let saved = self.save_version(i, "snap", &content);
        let discard = store::is_discard(&content);
        self.tabs[i].tr.reset(&content);
        if let Some(p) = self.tabs[i].d.path.clone() {
            if fs::write(&p, &content).is_ok() {
                self.tabs[i].d.saved = content;
            }
        }
        let n = self.version_count(&self.tabs[i].d.note_id.clone());
        self.set_status(if saved && discard {
            "Registrado como «descartado»".to_owned()
        } else if saved {
            format!("Snap guardado · {n} versiones")
        } else {
            "Sin cambios desde la última versión".into()
        });
        self.mark_dirty();
    }

    fn restore(&mut self, text: String) {
        let i = self.active;
        let cur = self.tabs[i].d.content.clone();
        self.save_version(i, "snap", &cur);
        let t = &mut self.tabs[i];
        t.d.content = text.clone();
        t.prev = text.clone();
        t.tr.reset(&text);
        self.preview = None;
        self.focus_editor = true;
        self.set_status("Versión restaurada (la anterior quedó como snap)");
        self.mark_dirty();
    }

    fn export_dataset(&mut self) {
        let out = self.store.root.join("pares.jsonl");
        match self.store.export_pairs(&out) {
            Ok(n) => {
                self.set_status(format!("{n} pares exportados a pares.jsonl"));
                let _ = std::process::Command::new("explorer").arg("/select,").arg(&out).spawn();
            }
            Err(e) => self.set_status(format!("Error exportando: {e}")),
        }
    }

    fn maybe_gc(&mut self) {
        if self.last_gc_check.elapsed() < Duration::from_secs(3600) {
            return;
        }
        self.last_gc_check = Instant::now();
        let today = chrono::Local::now().format("%Y%m%d").to_string();
        if self.settings.last_gc == today {
            return;
        }
        let open: HashSet<String> = self.tabs.iter().map(|t| t.d.note_id.clone()).collect();
        let packed = self.store.gc(&open, SystemTime::now(), GC_IDLE);
        self.settings.last_gc = today;
        self.save_settings();
        if packed > 0 {
            self.counts.clear();
            self.set_status(format!("Historial: {packed} notas inactivas empaquetadas"));
        }
    }

    // ── pestañas y archivos ──

    fn switch(&mut self, i: usize) {
        if i == self.active || i >= self.tabs.len() {
            return;
        }
        self.flush_tab(self.active);
        self.active = i;
        self.preview = None;
        self.focus_editor = true;
        self.mark_dirty();
    }

    fn new_tab(&mut self) {
        self.flush_tab(self.active);
        let id = self.next_id;
        self.next_id += 1;
        self.tabs.push(Tab::new(TabData {
            id,
            wrap: true,
            note_id: new_note_id(id),
            ..Default::default()
        }));
        self.active = self.tabs.len() - 1;
        self.preview = None;
        self.focus_editor = true;
        self.mark_dirty();
    }

    fn close_tab(&mut self, i: usize) {
        if self.tabs[i].file_dirty() {
            let ok = rfd::MessageDialog::new()
                .set_title("Cambios sin guardar")
                .set_description(format!("¿Descartar los cambios de «{}»?\n(Igual queda una copia en el historial.)", self.tabs[i].title()))
                .set_buttons(rfd::MessageButtons::YesNo)
                .set_level(rfd::MessageLevel::Warning)
                .show();
            if ok != rfd::MessageDialogResult::Yes {
                return;
            }
        }
        self.flush_tab(i);
        let content = self.tabs[i].d.content.clone();
        if self.save_version(i, "final", &content) {
            self.set_status("Pestaña cerrada · copia en el historial");
        }
        self.tabs.remove(i);
        if self.tabs.is_empty() {
            self.active = 0;
            let id = self.next_id;
            self.next_id += 1;
            self.tabs.push(Tab::new(TabData { id, wrap: true, note_id: new_note_id(id), ..Default::default() }));
        } else if self.active > i || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1).min(self.tabs.len() - 1);
        }
        self.preview = None;
        self.focus_editor = true;
        self.mark_dirty();
    }

    fn open_file(&mut self) {
        let Some(p) = rfd::FileDialog::new()
            .add_filter("Texto", &["txt", "md"])
            .add_filter("Todos", &["*"])
            .set_directory(&self.notes_dir)
            .pick_file()
        else {
            return;
        };
        let path = p.to_string_lossy().into_owned();
        let Ok(content) = fs::read_to_string(&p) else {
            self.set_status("No se pudo leer el archivo (¿no es texto?)");
            return;
        };
        if let Some(i) = self.tabs.iter().position(|t| t.d.path.as_deref() == Some(&path)) {
            if !self.tabs[i].file_dirty() {
                let t = &mut self.tabs[i];
                t.d.content = content.clone();
                t.d.saved = content.clone();
                t.prev = content.clone();
                t.tr.reset(&content);
            }
            self.switch(i);
            return;
        }
        self.flush_tab(self.active);
        let id = self.next_id;
        self.next_id += 1;
        self.tabs.push(Tab::new(TabData {
            id,
            title: file_name(&path).to_owned(),
            note_id: file_note_id(&path),
            path: Some(path),
            saved: content.clone(),
            content,
            wrap: true,
            ..Default::default()
        }));
        self.active = self.tabs.len() - 1;
        self.preview = None;
        self.focus_editor = true;
        self.mark_dirty();
    }

    fn save_file(&mut self, save_as: bool) {
        let i = self.active;
        let mut path = self.tabs[i].d.path.clone();
        if path.is_none() || save_as {
            let suggested = path
                .as_deref()
                .map(|p| file_name(p).to_owned())
                .unwrap_or_else(|| format!("{}.txt", sanitize(&self.tabs[i].title())));
            let start = path
                .as_deref()
                .and_then(|p| Path::new(p).parent().map(Path::to_path_buf))
                .unwrap_or_else(|| self.notes_dir.clone());
            let Some(p) = rfd::FileDialog::new()
                .add_filter("Texto", &["txt"])
                .add_filter("Markdown", &["md"])
                .add_filter("Todos", &["*"])
                .set_directory(start)
                .set_file_name(suggested)
                .save_file()
            else {
                return;
            };
            let mut p = p.to_string_lossy().into_owned();
            if !file_name(&p).contains('.') {
                p.push_str(".txt");
            }
            path = Some(p);
        }
        let path = path.unwrap();
        let t = &mut self.tabs[i];
        match fs::write(&path, &t.d.content) {
            Ok(()) => {
                t.d.saved = t.d.content.clone();
                t.d.title = file_name(&path).to_owned();
                t.d.path = Some(path.clone());
                self.set_status(format!("Guardado {path}"));
            }
            Err(e) => self.set_status(format!("Error al guardar: {e}")),
        }
        self.mark_dirty();
    }

    // ── corrección de transcripción (Ctrl+E) ──

    fn start_correction(&mut self) {
        let t = &self.tabs[self.active];
        if let Some((start, end, word)) = word_at(&t.d.content, t.cursor) {
            let paragraph = paragraph_at(&t.d.content, start).to_owned();
            self.correction = Some(Correction { start, end, new: word.clone(), word, paragraph, focus: true });
        }
    }

    fn commit_correction(&mut self) {
        let Some(c) = self.correction.take() else { return };
        let new = c.new.trim().to_owned();
        self.focus_editor = true;
        if new.is_empty() || new == c.word {
            return;
        }
        let i = self.active;
        let title = self.tabs[i].title();
        let t = &mut self.tabs[i];
        if t.d.content.get(c.start..c.end) != Some(c.word.as_str()) {
            return;
        }
        let old = t.d.content.clone();
        t.d.content.replace_range(c.start..c.end, &new);
        let cp = t.tr.on_edit(&old, &t.d.content, Instant::now());
        t.prev = t.d.content.clone();
        if let Some(cp) = cp {
            self.checkpoint(i, cp);
        }
        let path = self.dir.join("corrections.json");
        let mut list: Vec<serde_json::Value> = fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        list.push(serde_json::json!({
            "id": chrono::Utc::now().timestamp_millis(),
            "original": c.word,
            "corrected": new,
            "context": c.paragraph.trim(),
            "file": title,
            "timestamp": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        }));
        if let Ok(j) = serde_json::to_string_pretty(&list) {
            write_atomic(&path, &j);
        }
        self.set_status(format!("Corrección: «{}» → «{new}»", c.word));
        self.mark_dirty();
    }

    // ── atajos ──

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let ctrl = Modifiers::COMMAND;
        let sc = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)));

        if self.correction.is_some() {
            if ctx.input(|i| i.key_pressed(Key::Escape)) {
                self.correction = None;
                self.focus_editor = true;
            }
            return;
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) && self.preview.is_some() {
            self.preview = None;
            self.focus_editor = true;
        }
        // Las variantes con Shift/Alt van primero: Ctrl+S también "coincide" con Ctrl+Shift+S.
        if sc(ctrl | Modifiers::ALT, Key::S) {
            self.snap();
        } else if sc(ctrl | Modifiers::SHIFT, Key::S) {
            self.save_file(true);
        } else if sc(ctrl, Key::S) {
            self.save_file(false);
        }
        if sc(ctrl | Modifiers::SHIFT, Key::Tab) {
            let n = self.tabs.len();
            self.switch((self.active + n - 1) % n);
        } else if sc(ctrl, Key::Tab) {
            self.switch((self.active + 1) % self.tabs.len());
        }
        if sc(ctrl, Key::N) {
            self.new_tab();
        }
        if sc(ctrl, Key::O) {
            self.open_file();
        }
        if sc(ctrl, Key::W) {
            self.close_tab(self.active);
        }
        if sc(ctrl, Key::H) {
            self.history_open = !self.history_open;
        }
        if sc(ctrl, Key::E) {
            self.start_correction();
        }
        if sc(Modifiers::ALT, Key::Z) {
            let t = self.tab();
            t.d.wrap = !t.d.wrap;
            self.mark_dirty();
        }
        if sc(ctrl, Key::Equals) || sc(ctrl, Key::Plus) {
            self.set_font(self.settings.font_size + 1.0);
        }
        if sc(ctrl, Key::Minus) {
            self.set_font(self.settings.font_size - 1.0);
        }
        if sc(ctrl, Key::Num0) {
            self.set_font(FONT_DEFAULT);
        }
        let zoom = ctx.input(|i| i.zoom_delta());
        if zoom > 1.0 {
            self.set_font(self.settings.font_size + 1.0);
        } else if zoom < 1.0 {
            self.set_font(self.settings.font_size - 1.0);
        }
    }

    // ── UI ──

    fn top_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let bar = ui.max_rect();
        // Fondo arrastrable (los widgets que se dibujan después tienen prioridad).
        let drag = ui.interact(bar, Id::new("drag-bar"), Sense::click_and_drag());
        if drag.double_clicked() {
            let max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
            ctx.send_viewport_cmd(ViewportCommand::Maximized(!max));
        } else if drag.drag_started_by(egui::PointerButton::Primary) {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if window_button(ui, WinBtn::Close) {
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
            if window_button(ui, WinBtn::Max) {
                let max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(ViewportCommand::Maximized(!max));
            }
            if window_button(ui, WinBtn::Min) {
                ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
            }
            ui.add_space(4.0);
            ui.menu_button(RichText::new("≡").size(17.0), |ui| self.menu(ui, ctx));
            let hist = ui
                .selectable_label(self.history_open, RichText::new("Historial").size(12.5))
                .on_hover_text("Versiones de esta nota (Ctrl+H)");
            if hist.clicked() {
                self.history_open = !self.history_open;
            }
            if ui
                .button(RichText::new("Snap").size(12.5))
                .on_hover_text("Guardar versión ahora (Ctrl+Alt+S).\nEl historial también guarda solo antes y después de limpiar.")
                .clicked()
            {
                self.snap();
            }
            ui.add_space(6.0);

            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ScrollArea::horizontal()
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        let mut to_switch = None;
                        let mut to_close = None;
                        for (i, t) in self.tabs.iter().enumerate() {
                            match tab_widget(ui, &t.title(), i == self.active, t.file_dirty()) {
                                TabAction::Switch => to_switch = Some(i),
                                TabAction::Close => to_close = Some(i),
                                TabAction::None => {}
                            }
                        }
                        if ui.add(egui::Button::new(RichText::new("+").size(15.0)).frame(false)).on_hover_text("Nueva nota (Ctrl+N)").clicked() {
                            self.new_tab();
                        }
                        if let Some(i) = to_close {
                            self.close_tab(i);
                        } else if let Some(i) = to_switch {
                            self.switch(i);
                        }
                    });
            });
        });
    }

    fn menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.set_min_width(230.0);
        let item = |ui: &mut egui::Ui, text: &str, keys: &str| {
            ui.add(egui::Button::new(text).shortcut_text(keys)).clicked()
        };
        if item(ui, "Abrir…", "Ctrl+O") {
            ui.close_menu();
            self.open_file();
        }
        if item(ui, "Guardar", "Ctrl+S") {
            ui.close_menu();
            self.save_file(false);
        }
        if item(ui, "Guardar como…", "Ctrl+Shift+S") {
            ui.close_menu();
            self.save_file(true);
        }
        ui.separator();
        if item(ui, "Snap (versión ahora)", "Ctrl+Alt+S") {
            ui.close_menu();
            self.snap();
        }
        if item(ui, "Historial de esta nota", "Ctrl+H") {
            ui.close_menu();
            self.history_open = !self.history_open;
        }
        if item(ui, "Exportar dataset escrito→limpio", "") {
            ui.close_menu();
            self.export_dataset();
        }
        if item(ui, "Abrir carpeta del historial", "") {
            ui.close_menu();
            let _ = std::process::Command::new("explorer").arg(&self.store.root).spawn();
        }
        ui.separator();
        let wrap = self.tabs[self.active].d.wrap;
        if item(ui, if wrap { "✓ Ajuste de línea" } else { "   Ajuste de línea" }, "Alt+Z") {
            self.tab().d.wrap = !wrap;
            self.mark_dirty();
        }
        if item(ui, "Corregir palabra", "Ctrl+E") {
            ui.close_menu();
            self.start_correction();
        }
        ui.horizontal(|ui| {
            ui.label("Tema");
            let mut changed = false;
            for (val, label) in [("system", "Sistema"), ("light", "Claro"), ("dark", "Oscuro")] {
                if ui.selectable_label(self.settings.theme == val, label).clicked() {
                    self.settings.theme = val.into();
                    changed = true;
                }
            }
            if changed {
                self.apply_theme(ctx);
                self.save_settings();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Fuente");
            if ui.small_button("−").clicked() {
                self.set_font(self.settings.font_size - 1.0);
            }
            ui.label(format!("{} px", self.settings.font_size));
            if ui.small_button("+").clicked() {
                self.set_font(self.settings.font_size + 1.0);
            }
        });
        ui.separator();
        let stale = self.stats_cache.as_ref().map_or(true, |(t, _)| t.elapsed() > Duration::from_secs(10));
        if stale {
            let (n, v, b) = self.store.stats();
            self.stats_cache = Some((Instant::now(), format!("Historial: {n} notas · {v} versiones · {:.0} KB", b as f64 / 1024.0)));
        }
        ui.label(RichText::new(&self.stats_cache.as_ref().unwrap().1).small().weak());
    }

    fn history_panel(&mut self, ui: &mut egui::Ui) {
        let note = self.tabs[self.active].d.note_id.clone();
        if self.history_for != note {
            self.history = self.store.list(&note);
            self.history_for = note;
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Historial").strong());
            ui.label(RichText::new(format!("{}", self.history.len())).weak());
        });
        ui.label(
            RichText::new("Se guarda solo antes y después de limpiar, al cerrar y con Snap.")
                .small()
                .weak(),
        );
        ui.add_space(4.0);
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if self.history.is_empty() {
                ui.label(RichText::new("Aún no hay versiones.").weak());
            }
            let mut pick = None;
            for (i, v) in self.history.iter().enumerate().rev() {
                let sel = self.preview.as_ref().is_some_and(|(p, _)| *p == i);
                let label = format!("{}  {:<8} {}", short_ts(&v.ts), v.kind, human_bytes(v.bytes));
                if ui.selectable_label(sel, RichText::new(label).monospace().size(12.0)).clicked() {
                    pick = Some(i);
                }
            }
            if let Some(i) = pick {
                self.flush_tab(self.active);
                self.preview = Some((i, self.history[i].text()));
            }
        });
    }

    fn editor(&mut self, ui: &mut egui::Ui) {
        let font = FontId::monospace(self.settings.font_size);
        let i = self.active;
        let now = Instant::now();

        if let Some((idx, text)) = self.preview.clone() {
            let v = self.history.get(idx).cloned();
            let mut restore = false;
            let mut close = false;
            ui.horizontal(|ui| {
                if let Some(v) = &v {
                    ui.label(RichText::new(format!("Viendo {} · {}", short_ts(&v.ts), v.kind)).strong());
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    close = ui.button("Volver").clicked();
                    if ui.button("Copiar").clicked() {
                        ui.ctx().copy_text(text.clone());
                    }
                    restore = ui.button("Restaurar").on_hover_text("Reemplaza el texto actual (que queda guardado como snap)").clicked();
                });
            });
            ui.separator();
            ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.add(TextEdit::multiline(&mut text.as_str()).font(font).frame(false).desired_width(f32::INFINITY));
            });
            if restore {
                self.restore(text);
            } else if close {
                self.preview = None;
                self.focus_editor = true;
            }
            return;
        }

        let wrap = self.tabs[i].d.wrap;
        let color = ui.visuals().text_color();
        let mut layouter = |ui: &egui::Ui, s: &str, wrap_width: f32| {
            let job = LayoutJob::simple(s.to_owned(), font.clone(), color, if wrap { wrap_width } else { f32::INFINITY });
            ui.fonts(|f| f.layout_job(job))
        };
        let id = Id::new(("editor", self.tabs[i].d.id));
        if std::mem::take(&mut self.focus_editor) {
            ui.memory_mut(|m| m.request_focus(id));
        }
        let scroll = if wrap { ScrollArea::vertical() } else { ScrollArea::both() };
        let mut cp = None;
        let mut changed = false;
        scroll.auto_shrink([false, false]).show(ui, |ui| {
            let t = &mut self.tabs[i];
            let out = TextEdit::multiline(&mut t.d.content)
                .id(id)
                .frame(false)
                .desired_width(f32::INFINITY)
                .min_size(ui.available_size())
                .lock_focus(true)
                .layouter(&mut layouter)
                .show(ui);
            if let Some(c) = out.cursor_range {
                t.cursor = c.primary.ccursor.index;
            }
            if out.response.changed() {
                cp = t.tr.on_edit(&t.prev, &t.d.content, now);
                t.prev = t.d.content.clone();
                changed = true;
            }
        });
        if changed {
            self.mark_dirty();
        }
        if let Some(cp) = cp {
            self.checkpoint(i, cp);
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let note = self.tabs[self.active].d.note_id.clone();
        let n = self.version_count(&note);
        let t = &self.tabs[self.active];
        let (dot, phase) = match t.tr.phase {
            Phase::Writing => (ui.visuals().weak_text_color(), "escribiendo"),
            Phase::Cleaning => (accent(ui.visuals().dark_mode), "limpiando…"),
        };
        let chars = t.d.content.chars().count();
        ui.horizontal_centered(|ui| {
            ui.label(RichText::new("●").color(dot).size(10.0));
            ui.label(RichText::new(format!("{phase} · {n} versiones")).small().weak());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let msg = match &self.status {
                    Some((s, at)) if at.elapsed() < Duration::from_secs(5) => s.clone(),
                    _ => format!("{chars} car · {} px", self.settings.font_size),
                };
                ui.label(RichText::new(msg).small().weak());
            });
        });
    }

    fn correction_window(&mut self, ctx: &egui::Context) {
        let mut commit = false;
        let mut cancel = false;
        if let Some(c) = &mut self.correction {
            egui::Window::new("Corregir palabra")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(RichText::new(&c.word).strong());
                    let r = ui.add(TextEdit::singleline(&mut c.new).desired_width(260.0));
                    if std::mem::take(&mut c.focus) {
                        r.request_focus();
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        commit = true;
                    }
                    let ctx_preview: String = c.paragraph.chars().take(240).collect();
                    ui.label(RichText::new(ctx_preview).small().weak());
                    ui.horizontal(|ui| {
                        commit |= ui.button("Guardar").clicked();
                        cancel = ui.button("Cancelar").clicked();
                    });
                });
        }
        if commit {
            self.commit_correction();
        } else if cancel {
            self.correction = None;
            self.focus_editor = true;
        }
    }

    /// Bordes para redimensionar (la ventana no tiene marco nativo).
    fn resize_edges(&self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
            return;
        }
        let r = ctx.screen_rect();
        let Some(p) = ctx.input(|i| i.pointer.hover_pos()) else { return };
        let m = 5.0;
        let (l, rt, t, b) = (p.x < r.left() + m, p.x > r.right() - m, p.y < r.top() + 3.0, p.y > r.bottom() - m);
        let (dir, icon) = match (l, rt, t, b) {
            (true, _, true, _) => (ResizeDirection::NorthWest, CursorIcon::ResizeNorthWest),
            (_, true, true, _) => (ResizeDirection::NorthEast, CursorIcon::ResizeNorthEast),
            (true, _, _, true) => (ResizeDirection::SouthWest, CursorIcon::ResizeSouthWest),
            (_, true, _, true) => (ResizeDirection::SouthEast, CursorIcon::ResizeSouthEast),
            (true, ..) => (ResizeDirection::West, CursorIcon::ResizeWest),
            (_, true, ..) => (ResizeDirection::East, CursorIcon::ResizeEast),
            (_, _, true, _) => (ResizeDirection::North, CursorIcon::ResizeNorth),
            (_, _, _, true) => (ResizeDirection::South, CursorIcon::ResizeSouth),
            _ => return,
        };
        ctx.set_cursor_icon(icon);
        if ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.send_viewport_cmd(ViewportCommand::BeginResize(dir));
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if !self.setup_done {
            self.setup_done = true;
            round_corners(frame);
        }
        self.shortcuts(ctx);
        let dark = ctx.style().visuals.dark_mode;
        let (paper, bar) = palette(dark);

        egui::TopBottomPanel::top("bar")
            .exact_height(BAR_H)
            .frame(egui::Frame::none().fill(bar).inner_margin(Margin { left: 6.0, right: 0.0, top: 0.0, bottom: 0.0 }))
            .show(ctx, |ui| self.top_bar(ui, ctx));
        egui::TopBottomPanel::bottom("status")
            .exact_height(20.0)
            .frame(egui::Frame::none().fill(paper).inner_margin(Margin::symmetric(10.0, 0.0)))
            .show(ctx, |ui| self.status_bar(ui));
        if self.history_open {
            egui::SidePanel::right("history")
                .exact_width(210.0)
                .resizable(false)
                .frame(egui::Frame::none().fill(bar).inner_margin(Margin::symmetric(8.0, 4.0)))
                .show(ctx, |ui| self.history_panel(ui));
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(paper).inner_margin(Margin { left: 14.0, right: 6.0, top: 10.0, bottom: 4.0 }))
            .show(ctx, |ui| self.editor(ui));
        self.correction_window(ctx);
        self.resize_edges(ctx);

        // Fin de limpieza por inactividad, autoguardado de sesión y gc diario.
        let i = self.active;
        let content = self.tabs[i].d.content.clone();
        if let Some(cp) = self.tabs[i].tr.tick(&content, Instant::now()) {
            self.checkpoint(i, cp);
        }
        if self.session_dirty.is_some_and(|t| t.elapsed() > Duration::from_millis(1500)) {
            self.save_session();
        }
        self.maybe_gc();

        let mut wake = Duration::from_secs(60);
        if self.session_dirty.is_some() {
            wake = Duration::from_millis(1600);
        } else if self.tabs[i].tr.phase == Phase::Cleaning || self.status.is_some() {
            wake = Duration::from_secs(5);
        }
        ctx.request_repaint_after(wake);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        for i in 0..self.tabs.len() {
            self.flush_tab(i);
            let content = self.tabs[i].d.content.clone();
            self.save_version(i, "final", &content);
        }
        self.save_session();
        self.save_settings();
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        palette(visuals.dark_mode).0.to_normalized_gamma_f32()
    }
}

// ───────────────────────────── widgets ─────────────────────────────

fn accent(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0xe0, 0x8a, 0x5e)
    } else {
        Color32::from_rgb(0xc2, 0x60, 0x3a)
    }
}

/// (papel, barra)
fn palette(dark: bool) -> (Color32, Color32) {
    if dark {
        (Color32::from_rgb(0x1e, 0x1d, 0x1b), Color32::from_rgb(0x17, 0x16, 0x15))
    } else {
        (Color32::from_rgb(0xfb, 0xfa, 0xf7), Color32::from_rgb(0xef, 0xed, 0xe9))
    }
}

enum TabAction {
    None,
    Switch,
    Close,
}

fn tab_widget(ui: &mut egui::Ui, title: &str, active: bool, dirty: bool) -> TabAction {
    let font = FontId::proportional(12.5);
    let vis = ui.visuals().clone();
    let text_color = if active { vis.strong_text_color() } else { vis.weak_text_color() };
    let galley = ui.painter().layout_no_wrap(title.to_owned(), font, text_color);
    let size = Vec2::new(galley.size().x + 34.0, 24.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let close_rect = Rect::from_center_size(rect.right_center() - Vec2::new(12.0, 0.0), Vec2::splat(16.0));
    let hover_close = resp.hovered() && ui.input(|i| i.pointer.hover_pos()).is_some_and(|p| close_rect.contains(p));

    let painter = ui.painter();
    let (paper, _) = palette(vis.dark_mode);
    if active {
        painter.rect_filled(rect, Rounding::same(6.0), paper);
    } else if resp.hovered() {
        painter.rect_filled(rect, Rounding::same(6.0), vis.widgets.hovered.weak_bg_fill);
    }
    let text_pos = rect.left_center() + Vec2::new(10.0, -galley.size().y / 2.0);
    painter.galley(text_pos, galley, text_color);
    if dirty {
        painter.circle_filled(rect.left_center() + Vec2::new(4.5, 0.0), 2.5, accent(vis.dark_mode));
    }
    if active || resp.hovered() {
        if hover_close {
            painter.rect_filled(close_rect, Rounding::same(4.0), vis.widgets.hovered.bg_fill);
        }
        let c = close_rect.center();
        let s = 3.5;
        let stroke = Stroke::new(1.2, vis.weak_text_color());
        painter.line_segment([c - Vec2::splat(s), c + Vec2::splat(s)], stroke);
        painter.line_segment([c + Vec2::new(-s, s), c + Vec2::new(s, -s)], stroke);
    }
    if resp.middle_clicked() || (resp.clicked() && hover_close) {
        TabAction::Close
    } else if resp.clicked() {
        TabAction::Switch
    } else {
        TabAction::None
    }
}

enum WinBtn {
    Min,
    Max,
    Close,
}

fn window_button(ui: &mut egui::Ui, kind: WinBtn) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(40.0, BAR_H), Sense::click());
    let vis = ui.visuals();
    let painter = ui.painter();
    let mut color = vis.text_color();
    if resp.hovered() {
        if let WinBtn::Close = kind {
            painter.rect_filled(rect, Rounding::ZERO, Color32::from_rgb(0xc4, 0x2b, 0x1c));
            color = Color32::WHITE;
        } else {
            painter.rect_filled(rect, Rounding::ZERO, vis.widgets.hovered.weak_bg_fill);
        }
    }
    let c = rect.center();
    let s = 5.0;
    let stroke = Stroke::new(1.0, color);
    match kind {
        WinBtn::Min => {
            painter.line_segment([c + Vec2::new(-s, 0.0), c + Vec2::new(s, 0.0)], stroke);
        }
        WinBtn::Max => {
            painter.rect_stroke(Rect::from_center_size(c, Vec2::splat(2.0 * s - 1.0)), Rounding::same(1.0), stroke);
        }
        WinBtn::Close => {
            painter.line_segment([c - Vec2::splat(s), c + Vec2::splat(s)], stroke);
            painter.line_segment([c + Vec2::new(-s, s), c + Vec2::new(s, -s)], stroke);
        }
    }
    resp.clicked()
}

fn short_ts(ts: &str) -> String {
    // 20261113-095135 → 13/11 09:51
    if ts.len() >= 13 {
        format!("{}/{} {}:{}", &ts[6..8], &ts[4..6], &ts[9..11], &ts[11..13])
    } else {
        ts.to_owned()
    }
}

fn human_bytes(b: usize) -> String {
    if b < 1024 {
        format!("{b} B")
    } else {
        format!("{:.1} KB", b as f64 / 1024.0)
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '\'' || c == '-' || c == '_'
}

/// Palabra alrededor del cursor (índice en caracteres) → (inicio, fin en bytes, palabra).
fn word_at(text: &str, char_idx: usize) -> Option<(usize, usize, String)> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    if chars.is_empty() {
        return None;
    }
    let mut i = char_idx.min(chars.len());
    if i == chars.len() || !is_word_char(chars[i].1) {
        if i > 0 && is_word_char(chars[i - 1].1) {
            i -= 1;
        } else {
            return None;
        }
    }
    let mut s = i;
    while s > 0 && is_word_char(chars[s - 1].1) {
        s -= 1;
    }
    let mut e = i;
    while e < chars.len() && is_word_char(chars[e].1) {
        e += 1;
    }
    let start = chars[s].0;
    let end = if e < chars.len() { chars[e].0 } else { text.len() };
    Some((start, end, text[start..end].to_owned()))
}

/// Párrafo (bloque entre líneas en blanco) que contiene el byte `pos`.
fn paragraph_at(text: &str, pos: usize) -> &str {
    let start = text[..pos].rfind("\n\n").map(|i| i + 2).unwrap_or(0);
    let end = text[pos..].find("\n\n").map(|i| pos + i).unwrap_or(text.len());
    &text[start..end]
}

// ───────────────────────────── arranque ─────────────────────────────

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let fdir = PathBuf::from(std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into())).join("Fonts");
    let mut add = |name: &str, file: &str, family: FontFamily, front: bool| {
        if let Ok(bytes) = fs::read(fdir.join(file)) {
            fonts.font_data.insert(name.into(), FontData::from_owned(bytes).into());
            let list = fonts.families.entry(family).or_default();
            if front {
                list.insert(0, name.into());
            } else {
                list.push(name.into());
            }
        }
    };
    add("segoe", "segoeui.ttf", FontFamily::Proportional, true);
    add("consolas", "consola.ttf", FontFamily::Monospace, true);
    add("symbols", "seguisym.ttf", FontFamily::Proportional, false);
    add("symbols-m", "seguisym.ttf", FontFamily::Monospace, false);
    ctx.set_fonts(fonts);
}

fn setup_style(ctx: &egui::Context) {
    for theme in [Theme::Light, Theme::Dark] {
        ctx.style_mut_of(theme, |s| {
            let dark = theme == Theme::Dark;
            let (paper, bar) = palette(dark);
            s.spacing.button_padding = Vec2::new(8.0, 3.0);
            s.visuals.panel_fill = paper;
            s.visuals.window_fill = paper;
            s.visuals.extreme_bg_color = paper;
            s.visuals.selection.bg_fill = accent(dark).gamma_multiply(0.35);
            s.visuals.selection.stroke = Stroke::new(1.0, accent(dark));
            s.visuals.text_cursor.stroke = Stroke::new(2.0, accent(dark));
            // Botones planos: solo se notan al pasar el mouse.
            s.visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
            s.visuals.widgets.inactive.bg_stroke = Stroke::NONE;
            s.visuals.widgets.hovered.bg_stroke = Stroke::NONE;
            s.visuals.widgets.active.bg_stroke = Stroke::NONE;
            s.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, bar);
            s.visuals.window_stroke = Stroke::new(1.0, if dark { Color32::from_gray(60) } else { Color32::from_gray(200) });
        });
    }
}

#[cfg(windows)]
fn round_corners(frame: &eframe::Frame) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND};
    if let Ok(h) = frame.window_handle() {
        if let RawWindowHandle::Win32(w) = h.as_raw() {
            let pref = DWMWCP_ROUND;
            unsafe {
                DwmSetWindowAttribute(
                    w.hwnd.get() as _,
                    DWMWA_WINDOW_CORNER_PREFERENCE as u32,
                    &pref as *const _ as *const _,
                    std::mem::size_of_val(&pref) as u32,
                );
            }
        }
    }
}

#[cfg(not(windows))]
fn round_corners(_frame: &eframe::Frame) {}

/// Igual que la versión Tauri: acoplada a la derecha del área de trabajo.
/// Devuelve (pos, tamaño) en puntos lógicos.
#[cfg(windows)]
fn right_side_placement() -> Option<([f32; 2], [f32; 2])> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::HiDpi::{GetDpiForSystem, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
    use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETWORKAREA};
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let mut r: RECT = std::mem::zeroed();
        if SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut r as *mut _ as *mut _, 0) == 0 {
            return None;
        }
        let s = GetDpiForSystem() as f32 / 96.0;
        let (ax, ay) = (r.left as f32 / s, r.top as f32 / s);
        let (aw, ah) = ((r.right - r.left) as f32 / s, (r.bottom - r.top) as f32 / s);
        let m = 12.0;
        let w = (aw * 0.42).clamp(400.0, 560.0).min(aw - 2.0 * m);
        let h = (ah * 0.92).clamp(300.0, 980.0).min(ah - 2.0 * m);
        Some(([ax + aw - w - m, ay + (ah - h) / 2.0], [w, h]))
    }
}

#[cfg(not(windows))]
fn right_side_placement() -> Option<([f32; 2], [f32; 2])> {
    None
}

fn main() -> eframe::Result {
    // `scratchpad --dataset [salida.jsonl]`: exporta los pares escrito→limpio y sale.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--dataset") {
        let store = Store::new(data_dir().join("history"));
        let out = args.get(2).map(PathBuf::from).unwrap_or_else(|| store.root.join("pares.jsonl"));
        let _ = store.export_pairs(&out);
        return Ok(());
    }

    let mut vp = egui::ViewportBuilder::default()
        .with_title("Scratchpad")
        .with_decorations(false)
        .with_resizable(true)
        .with_min_inner_size([360.0, 300.0])
        .with_inner_size([520.0, 720.0]);
    if let Some((pos, size)) = right_side_placement() {
        vp = vp.with_position(pos).with_inner_size(size);
    }
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../../src-tauri/icons/128x128.png")) {
        vp = vp.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport: vp,
        ..Default::default()
    };
    eframe::run_native("Scratchpad", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_and_paragraph() {
        let t = "hola mundo\n\nel quitar source";
        let (s, e, w) = word_at(t, 15).unwrap();
        assert_eq!(w, "quitar");
        assert_eq!(&t[s..e], "quitar");
        assert_eq!(paragraph_at(t, s), "el quitar source");
        assert_eq!(word_at("ñandú feliz", 5).unwrap().2, "ñandú");
        assert!(word_at("   ", 1).is_none());
    }

    #[test]
    fn note_ids_are_stable() {
        assert_eq!(file_note_id("C:\\a\\Mi nota.txt"), file_note_id("c:\\A\\mi NOTA.txt").replace("mi_NOTA", "Mi_nota"));
        assert!(file_note_id("C:\\a\\x.txt").starts_with("f-x-"));
    }

    #[test]
    fn tauri_session_is_readable() {
        let j = r#"{"activeId":46,"nextId":47,"tabs":[{"id":46,"title":"a.txt","path":null,"content":"hola","saved":"","threadBase":"a","snapN":1,"wrap":true}]}"#;
        let s: Session = serde_json::from_str(j).unwrap();
        assert_eq!(s.tabs[0].content, "hola");
        assert!(s.tabs[0].note_id.is_empty());
    }
}
