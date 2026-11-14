//! Detector barato de fases "escribiendo" → "limpiando".
//!
//! No analiza el texto: por cada edición solo mide cuántos caracteres entraron
//! y salieron (prefijo/sufijo común). Con eso decide cuándo empezó una limpieza
//! (para guardar el estado "escrito") y cuándo terminó (para guardar "limpio").

use std::time::{Duration, Instant};

/// Un borrado de una sola vez de este tamaño (seleccionar + suprimir/cortar) = limpieza.
const BIG_DELETE: usize = 40;
/// Borrado acumulado reciente que también cuenta como limpieza…
const CLEAN_DEL: f32 = 80.0;
/// …si además domina claramente sobre lo escrito en la misma ventana.
const CLEAN_RATIO: f32 = 1.0;
/// Vida media de los contadores (ventana deslizante "reciente").
const HALF_LIFE_S: f32 = 45.0;
/// Si durante la limpieza vuelves a escribir mucho, la limpieza terminó.
const RESUME_INS: f32 = 300.0;
/// Sin tocar la nota este tiempo en fase de limpieza → se da por terminada.
pub const IDLE_END: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Writing,
    Cleaning,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Checkpoint {
    /// Estado justo antes de empezar a limpiar.
    Escrito(String),
    /// Estado al terminar de limpiar.
    Limpio(String),
}

pub struct Tracker {
    pub phase: Phase,
    /// Último estado tras una inserción en fase de escritura.
    peak: String,
    /// Último estado tras un borrado en fase de limpieza.
    clean_state: String,
    ins: f32,
    del: f32,
    last_edit: Option<Instant>,
}

/// Caracteres (no bytes) quitados e insertados entre `old` y `new`.
pub fn edit_size(old: &str, new: &str) -> (usize, usize) {
    let prefix: usize = old
        .chars()
        .zip(new.chars())
        .take_while(|(a, b)| a == b)
        .count();
    let old_n = old.chars().count();
    let new_n = new.chars().count();
    let max_suffix = old_n.min(new_n) - prefix;
    let suffix = old
        .chars()
        .rev()
        .zip(new.chars().rev())
        .take(max_suffix)
        .take_while(|(a, b)| a == b)
        .count();
    (old_n - prefix - suffix, new_n - prefix - suffix)
}

impl Tracker {
    pub fn new(current: &str) -> Self {
        Self {
            phase: Phase::Writing,
            peak: current.to_owned(),
            clean_state: String::new(),
            ins: 0.0,
            del: 0.0,
            last_edit: None,
        }
    }

    fn decay(&mut self, now: Instant) {
        if let Some(t) = self.last_edit {
            let dt = now.saturating_duration_since(t).as_secs_f32();
            let f = 0.5f32.powf(dt / HALF_LIFE_S);
            self.ins *= f;
            self.del *= f;
        }
    }

    /// Llamar con el texto antes y después de cada cambio.
    pub fn on_edit(&mut self, old: &str, new: &str, now: Instant) -> Option<Checkpoint> {
        let (removed, inserted) = edit_size(old, new);
        self.decay(now);
        self.last_edit = Some(now);
        self.ins += inserted as f32;
        self.del += removed as f32;

        match self.phase {
            Phase::Writing => {
                let cleaning = removed >= BIG_DELETE
                    || (self.del >= CLEAN_DEL && self.del > CLEAN_RATIO * self.ins);
                if cleaning {
                    self.phase = Phase::Cleaning;
                    self.clean_state = new.to_owned();
                    // Si nunca hubo inserción (nota recién abierta), lo escrito es `old`.
                    let escrito = if self.peak.is_empty() { old } else { &self.peak };
                    return Some(Checkpoint::Escrito(escrito.to_owned()));
                }
                if inserted > 0 && inserted >= removed {
                    self.peak = new.to_owned();
                }
                None
            }
            Phase::Cleaning => {
                if self.ins >= RESUME_INS && self.ins > 3.0 * self.del {
                    // Volviste a escribir: lo limpio es el estado antes de esta racha.
                    let limpio = std::mem::take(&mut self.clean_state);
                    self.reset(new);
                    return Some(Checkpoint::Limpio(limpio));
                }
                if removed > 0 {
                    self.clean_state = new.to_owned();
                }
                None
            }
        }
    }

    /// Llamar periódicamente: cierra la limpieza tras un rato sin tocar la nota.
    pub fn tick(&mut self, current: &str, now: Instant) -> Option<Checkpoint> {
        if self.phase == Phase::Cleaning {
            if let Some(t) = self.last_edit {
                if now.saturating_duration_since(t) >= IDLE_END {
                    self.reset(current);
                    return Some(Checkpoint::Limpio(current.to_owned()));
                }
            }
        }
        None
    }

    /// Cambio de pestaña, cierre o salida: si estabas limpiando, eso es lo limpio.
    pub fn flush(&mut self, current: &str) -> Option<Checkpoint> {
        if self.phase == Phase::Cleaning {
            self.reset(current);
            return Some(Checkpoint::Limpio(current.to_owned()));
        }
        None
    }

    /// Tras un Snap manual o restaurar una versión.
    pub fn reset(&mut self, current: &str) {
        *self = Self::new(current);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> Instant {
        Instant::now()
    }

    #[test]
    fn edit_size_basics() {
        assert_eq!(edit_size("hola", "hola mundo"), (0, 6));
        assert_eq!(edit_size("hola mundo", "hola"), (6, 0));
        assert_eq!(edit_size("aXc", "aYYc"), (1, 2));
        assert_eq!(edit_size("ñandú", "ñandú!"), (0, 1));
        assert_eq!(edit_size("aaa", "aa"), (1, 0));
    }

    #[test]
    fn typing_with_typos_is_not_cleaning() {
        let now = t0();
        let mut tr = Tracker::new("");
        let mut text = String::new();
        for i in 0..400 {
            let old = text.clone();
            if i % 10 == 9 {
                text.pop();
            } else {
                text.push('a');
            }
            assert_eq!(tr.on_edit(&old, &text, now + Duration::from_millis(i * 150)), None);
        }
        assert_eq!(tr.phase, Phase::Writing);
    }

    #[test]
    fn big_selection_delete_saves_escrito_then_idle_saves_limpio() {
        let now = t0();
        let mut tr = Tracker::new("");
        let full = "x".repeat(500);
        assert_eq!(tr.on_edit("", &full, now), None);
        let cleaned = "x".repeat(400);
        let cp = tr.on_edit(&full, &cleaned, now + Duration::from_secs(5));
        assert_eq!(cp, Some(Checkpoint::Escrito(full.clone())));
        assert_eq!(tr.phase, Phase::Cleaning);
        assert_eq!(tr.tick(&cleaned, now + Duration::from_secs(60)), None);
        let cp = tr.tick(&cleaned, now + Duration::from_secs(5) + IDLE_END);
        assert_eq!(cp, Some(Checkpoint::Limpio(cleaned)));
        assert_eq!(tr.phase, Phase::Writing);
    }

    #[test]
    fn many_backspaces_trigger_cleaning_with_pre_state() {
        let now = t0();
        let mut tr = Tracker::new("");
        let full = "palabra ".repeat(60);
        tr.on_edit("", &full, now);
        let mut text = full.clone();
        let mut got = None;
        for i in 0..300 {
            let old = text.clone();
            text.pop();
            if let Some(cp) = tr.on_edit(&old, &text, now + Duration::from_secs(70) + Duration::from_millis(i * 80)) {
                got = Some(cp);
                break;
            }
        }
        assert_eq!(got, Some(Checkpoint::Escrito(full)));
    }

    #[test]
    fn resuming_writing_closes_cleaning() {
        let now = t0();
        let mut tr = Tracker::new("");
        let full = "y".repeat(600);
        tr.on_edit("", &full, now);
        let cleaned = "y".repeat(300);
        tr.on_edit(&full, &cleaned, now + Duration::from_secs(1));
        let more = format!("{cleaned}{}", "z".repeat(400));
        let cp = tr.on_edit(&cleaned, &more, now + Duration::from_secs(2));
        assert_eq!(cp, Some(Checkpoint::Limpio(cleaned)));
        assert_eq!(tr.phase, Phase::Writing);
    }

    #[test]
    fn flush_only_when_cleaning() {
        let mut tr = Tracker::new("abc");
        assert_eq!(tr.flush("abc"), None);
    }
}
