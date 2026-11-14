//! Historial de versiones = dataset de cómo escribes y limpias.
//!
//! Disposición (todo texto plano, fuera de tus carpetas de trabajo):
//!
//! ```text
//! %LOCALAPPDATA%\scratchpad\history\
//!   <nota>\20261113-095135_escrito.txt   ← notas activas: un .txt por versión
//!   <nota>\20261113-095410_limpio.txt
//!   packs\2026-11.jsonl                   ← notas inactivas, empaquetadas por mes
//!   pares.jsonl                           ← export "escrito → limpio" (--dataset)
//! ```
//!
//! Tipos de versión: `escrito` (antes de limpiar), `limpio` (después),
//! `snap` (manual), `final` (al cerrar la pestaña/app con cambios).
//! Nunca se guarda una versión idéntica a la anterior de la misma nota.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Hash FNV-1a: estable entre ejecuciones y versiones de Rust (sirve para ids).
pub fn fnv(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Vacío, o 1–3 caracteres sin letras ni números (tu "." de "no sirve nada").
pub fn is_discard(text: &str) -> bool {
    let t = text.trim();
    t.chars().count() <= 3 && !t.chars().any(char::is_alphanumeric)
}

pub fn now_ts() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

#[derive(Clone, Debug)]
pub enum Src {
    File(PathBuf),
    Packed(String),
}

#[derive(Clone, Debug)]
pub struct Version {
    pub ts: String,
    pub kind: String,
    pub bytes: usize,
    pub src: Src,
}

impl Version {
    pub fn text(&self) -> String {
        match &self.src {
            Src::File(p) => fs::read_to_string(p).unwrap_or_default(),
            Src::Packed(t) => t.clone(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct PackLine {
    note: String,
    ts: String,
    kind: String,
    text: String,
}

#[derive(Serialize)]
struct Pair<'a> {
    note: &'a str,
    ts_antes: &'a str,
    ts_despues: &'a str,
    tipo_antes: &'a str,
    tipo_despues: &'a str,
    chars_antes: usize,
    chars_despues: usize,
    antes: String,
    despues: String,
}

pub struct Store {
    pub root: PathBuf,
    last_hash: HashMap<String, u64>,
}

/// "20261113-095135-2_limpio.txt" → ("20261113-095135-2", "limpio")
fn parse_name(name: &str) -> Option<(String, String)> {
    let stem = name.strip_suffix(".txt")?;
    let (ts, kind) = stem.split_once('_')?;
    if ts.len() < 15 || !ts.as_bytes()[..8].iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some((ts.to_owned(), kind.to_owned()))
}

impl Store {
    pub fn new(root: PathBuf) -> Self {
        let _ = fs::create_dir_all(root.join("packs"));
        Self {
            root,
            last_hash: HashMap::new(),
        }
    }

    fn packs_dir(&self) -> PathBuf {
        self.root.join("packs")
    }

    /// Guarda una versión. Devuelve la ruta, o None si era idéntica/vacía.
    /// Si queda vacío o solo signos (".", "-", …) y antes había texto, se guarda
    /// como `descartado` (archivo vacío): "borré todo" también es un dato.
    pub fn save(&mut self, note: &str, kind: &str, text: &str, ts: &str) -> Option<PathBuf> {
        let (kind, text) = if is_discard(text) { ("descartado", "") } else { (kind, text) };
        let h = fnv(text);
        let last = match self.last_hash.get(note) {
            Some(h) => Some(*h),
            None => self.list(note).last().map(|v| fnv(&v.text())),
        };
        // Nota nueva que nunca tuvo texto: nada que registrar.
        if kind == "descartado" && last.is_none() {
            return None;
        }
        if last == Some(h) {
            self.last_hash.insert(note.to_owned(), h);
            return None;
        }
        let dir = self.root.join(note);
        fs::create_dir_all(&dir).ok()?;
        let mut path = dir.join(format!("{ts}_{kind}.txt"));
        let mut n = 2;
        while path.exists() {
            path = dir.join(format!("{ts}-{n}_{kind}.txt"));
            n += 1;
        }
        fs::write(&path, text).ok()?;
        self.last_hash.insert(note.to_owned(), h);
        Some(path)
    }

    /// Versiones de una nota (sueltas + empaquetadas), de la más vieja a la más nueva.
    pub fn list(&self, note: &str) -> Vec<Version> {
        let mut out = Vec::new();
        if let Ok(rd) = fs::read_dir(self.root.join(note)) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if let Some((ts, kind)) = parse_name(&name) {
                    let bytes = e.metadata().map(|m| m.len() as usize).unwrap_or(0);
                    out.push(Version {
                        ts,
                        kind,
                        bytes,
                        src: Src::File(e.path()),
                    });
                }
            }
        }
        for line in self.pack_lines() {
            if line.note == note {
                out.push(Version {
                    ts: line.ts,
                    kind: line.kind,
                    bytes: line.text.len(),
                    src: Src::Packed(line.text),
                });
            }
        }
        out.sort_by(|a, b| a.ts.cmp(&b.ts));
        out
    }

    fn pack_lines(&self) -> Vec<PackLine> {
        let mut out = Vec::new();
        let Ok(rd) = fs::read_dir(self.packs_dir()) else {
            return out;
        };
        let mut files: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        files.sort();
        for p in files {
            if p.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            if let Ok(f) = fs::File::open(&p) {
                for line in io::BufReader::new(f).lines().map_while(Result::ok) {
                    if let Ok(l) = serde_json::from_str::<PackLine>(&line) {
                        out.push(l);
                    }
                }
            }
        }
        out
    }

    fn note_dirs(&self) -> Vec<(String, PathBuf)> {
        let Ok(rd) = fs::read_dir(&self.root) else {
            return Vec::new();
        };
        rd.flatten()
            .filter(|e| e.path().is_dir() && e.file_name() != "packs")
            .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
            .collect()
    }

    /// "git gc": notas sin cambios hace `idle` y que no están abiertas se empaquetan
    /// en packs/AAAA-MM.jsonl (sin duplicados consecutivos) y se borra su carpeta.
    /// Solo lista directorios y lee textos pequeños: barato, una vez al día.
    pub fn gc(&mut self, open: &HashSet<String>, now: SystemTime, idle: Duration) -> usize {
        let mut packed = 0;
        for (note, dir) in self.note_dirs() {
            if open.contains(&note) {
                continue;
            }
            let newest = fs::read_dir(&dir)
                .map(|rd| {
                    rd.flatten()
                        .filter_map(|e| e.metadata().ok()?.modified().ok())
                        .max()
                })
                .ok()
                .flatten();
            let old_enough = match newest {
                Some(t) => now.duration_since(t).unwrap_or_default() >= idle,
                None => true,
            };
            if old_enough && self.pack_note(&note, &dir).is_ok() {
                packed += 1;
            }
        }
        packed
    }

    fn pack_note(&mut self, note: &str, dir: &Path) -> io::Result<()> {
        let mut by_month: BTreeMap<String, Vec<PackLine>> = BTreeMap::new();
        let mut prev: Option<u64> = None;
        for v in self.list(note) {
            let Src::File(_) = v.src else { continue };
            let text = v.text();
            let h = fnv(&text);
            if prev == Some(h) {
                continue;
            }
            prev = Some(h);
            let month = format!("{}-{}", &v.ts[..4], &v.ts[4..6]);
            by_month.entry(month).or_default().push(PackLine {
                note: note.to_owned(),
                ts: v.ts,
                kind: v.kind,
                text,
            });
        }
        for (month, lines) in by_month {
            let mut f = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.packs_dir().join(format!("{month}.jsonl")))?;
            for l in lines {
                writeln!(f, "{}", serde_json::to_string(&l)?)?;
            }
        }
        fs::remove_dir_all(dir)?;
        self.last_hash.remove(note);
        Ok(())
    }

    /// Dataset para analizar: cada `escrito`/`snap` con la versión siguiente.
    pub fn export_pairs(&self, out: &Path) -> io::Result<usize> {
        let mut notes: HashSet<String> = self.note_dirs().into_iter().map(|(n, _)| n).collect();
        notes.extend(self.pack_lines().into_iter().map(|l| l.note));
        let mut notes: Vec<_> = notes.into_iter().collect();
        notes.sort();
        let mut f = io::BufWriter::new(fs::File::create(out)?);
        let mut n = 0;
        for note in &notes {
            let vs = self.list(note);
            for w in vs.windows(2) {
                let (a, b) = (&w[0], &w[1]);
                if !(a.kind == "escrito" || a.kind == "snap") || b.kind == "escrito" {
                    continue;
                }
                let (ta, tb) = (a.text(), b.text());
                let pair = Pair {
                    note,
                    ts_antes: &a.ts,
                    ts_despues: &b.ts,
                    tipo_antes: &a.kind,
                    tipo_despues: &b.kind,
                    chars_antes: ta.chars().count(),
                    chars_despues: tb.chars().count(),
                    antes: ta,
                    despues: tb,
                };
                writeln!(f, "{}", serde_json::to_string(&pair)?)?;
                n += 1;
            }
        }
        Ok(n)
    }

    /// (notas, versiones, bytes) para mostrar en el menú.
    pub fn stats(&self) -> (usize, usize, u64) {
        let mut notes = HashSet::new();
        let (mut files, mut bytes) = (0, 0u64);
        for (note, dir) in self.note_dirs() {
            notes.insert(note);
            for e in fs::read_dir(dir).into_iter().flatten().flatten() {
                files += 1;
                bytes += e.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
        for l in self.pack_lines() {
            notes.insert(l.note);
            files += 1;
            bytes += l.text.len() as u64;
        }
        (notes.len(), files, bytes)
    }

    /// Importa una sola vez los viejos `base_vN.txt` de la app Tauri como `snap`.
    pub fn import_tauri_snaps(&mut self, notes_dir: &Path) {
        let stamp = self.root.join(".tauri_importado");
        if stamp.exists() {
            return;
        }
        let mut found: Vec<(String, u32, PathBuf)> = Vec::new();
        for e in fs::read_dir(notes_dir).into_iter().flatten().flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Some(stem) = name.strip_suffix(".txt") else { continue };
            let Some((base, n)) = stem.rsplit_once("_v") else { continue };
            if let Ok(n) = n.parse::<u32>() {
                found.push((base.to_owned(), n, e.path()));
            }
        }
        found.sort();
        for (base, _, path) in found {
            let ts = fs::metadata(&path)
                .and_then(|m| m.modified())
                .map(|t| chrono::DateTime::<chrono::Local>::from(t).format("%Y%m%d-%H%M%S").to_string())
                .unwrap_or_else(|_| now_ts());
            if let Ok(text) = fs::read_to_string(&path) {
                self.save(&format!("t-{base}"), "snap", &text, &ts);
            }
        }
        let _ = fs::write(stamp, now_ts());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("scratchpad_store_{name}"));
        let _ = fs::remove_dir_all(&p);
        p
    }

    #[test]
    fn save_dedups_and_lists_in_order() {
        let root = tmp("dedup");
        let mut s = Store::new(root.clone());
        assert!(s.save("n1", "escrito", "texto largo", "20261113-100000").is_some());
        assert!(s.save("n1", "limpio", "texto largo", "20261113-100100").is_none());
        assert!(s.save("n1", "limpio", "texto", "20261113-100200").is_some());
        // mismo segundo → sufijo, sin pisar
        assert!(s.save("n1", "snap", "otro", "20261113-100200").is_some());
        let v = s.list("n1");
        assert_eq!(v.len(), 3);
        // nota nueva vacía: no se registra nada
        assert!(s.save("n2", "final", "", "20261113-100300").is_none());
        assert!(s.save("n2", "final", ".", "20261113-100300").is_none());
        assert_eq!(v[0].kind, "escrito");
        assert_eq!(v[1].text(), "texto");
        // nueva instancia: el dedup lee la última versión del disco
        let mut s2 = Store::new(root.clone());
        assert!(s2.save("n1", "final", "otro", "20261113-100400").is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn gc_packs_idle_notes_and_pairs_export() {
        let root = tmp("gc");
        let mut s = Store::new(root.clone());
        s.save("viva", "escrito", "abc def", "20261113-100000");
        s.save("vieja", "escrito", "uno dos tres", "20261013-100000");
        s.save("vieja", "limpio", "uno tres", "20261013-100500");
        let open: HashSet<String> = ["viva".to_owned()].into();
        let packed = s.gc(&open, SystemTime::now() + Duration::from_secs(8 * 86400), Duration::from_secs(7 * 86400));
        assert_eq!(packed, 1);
        assert!(!root.join("vieja").exists());
        assert!(root.join("viva").exists());
        assert!(root.join("packs").join("2026-10.jsonl").exists());
        let v = s.list("vieja");
        assert_eq!(v.len(), 2);
        assert_eq!(v[1].text(), "uno tres");

        let out = root.join("pares.jsonl");
        assert_eq!(s.export_pairs(&out).unwrap(), 1);
        let line = fs::read_to_string(&out).unwrap();
        assert!(line.contains("\"antes\":\"uno dos tres\""));
        assert!(line.contains("\"despues\":\"uno tres\""));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn deleting_everything_is_recorded_once() {
        let root = tmp("discard");
        let mut s = Store::new(root.clone());
        s.save("n", "escrito", "mucho texto inútil", "20261113-100000");
        let p = s.save("n", "limpio", ".", "20261113-100100").unwrap();
        assert!(p.to_string_lossy().ends_with("_descartado.txt"));
        assert_eq!(fs::read_to_string(&p).unwrap(), "");
        assert!(s.save("n", "final", "", "20261113-100200").is_none()); // ya registrado
        assert!(s.save("n", "snap", "a", "20261113-100300").is_some()); // "a" es texto
        let out = root.join("pares.jsonl");
        assert_eq!(s.export_pairs(&out).unwrap(), 1);
        assert!(fs::read_to_string(&out).unwrap().contains("\"tipo_despues\":\"descartado\""));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parse_names() {
        assert_eq!(parse_name("20261113-095135_limpio.txt"), Some(("20261113-095135".into(), "limpio".into())));
        assert_eq!(parse_name("20261113-095135-2_snap.txt"), Some(("20261113-095135-2".into(), "snap".into())));
        assert_eq!(parse_name("notas.txt"), None);
    }
}
