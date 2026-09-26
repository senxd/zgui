//! Identical bounded deterministic work and geometry for all three adapters.
use std::{fmt::Write, ops::Range, time::Duration};
pub const WIDTH: f32 = 960.0;
pub const HEIGHT: f32 = 720.0;
pub const ROW_HEIGHT: f32 = 28.0;
pub const ROW_COUNT: usize = 100_000;
pub const VIEWPORT_HEIGHT: f32 = 400.0;
pub const PERIOD: Duration = Duration::from_nanos(16_666_667);
pub const TOKEN: &str = "The quick brown fox streams a token. ";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Idle,
    Stream,
    Scroll,
    Both,
}
impl Mode {
    pub fn from_env() -> Self {
        match std::env::var("ZGUI_MODE").as_deref() {
            Ok("idle") => Self::Idle,
            Ok("stream") => Self::Stream,
            Ok("scroll") => Self::Scroll,
            _ => Self::Both,
        }
    }
}
pub struct Workload {
    pub text: String,
    pub scroll_offset: f32,
    pub frames: u64,
    pub mode: Mode,
    pub seconds: f64,
}
impl Default for Workload {
    fn default() -> Self {
        Self::from_env()
    }
}
impl Workload {
    pub fn from_env() -> Self {
        let mut work = Self {
            text: String::with_capacity(8256),
            scroll_offset: 0.,
            frames: 0,
            mode: Mode::from_env(),
            seconds: std::env::var("ZGUI_SECONDS")
                .ok()
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| v.is_finite() && *v >= 0.)
                .unwrap_or(30.),
        };
        let initial_ticks = std::env::var("ZGUI_INITIAL_TICKS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
            .min(100_000);
        let mode = work.mode;
        work.mode = Mode::Both;
        for _ in 0..initial_ticks {
            work.tick();
        }
        work.mode = mode;
        work
    }
    pub fn tick(&mut self) {
        self.frames += 1;
        if matches!(self.mode, Mode::Stream | Mode::Both) {
            write!(self.text, "{:06} {TOKEN}", self.frames).unwrap();
            if self.text.len() > 8192 {
                self.text.drain(..self.text.len() - 8192);
            }
        }
        if matches!(self.mode, Mode::Scroll | Mode::Both) {
            self.scroll_offset =
                (self.scroll_offset + 14.) % (ROW_COUNT as f32 * ROW_HEIGHT - VIEWPORT_HEIGHT);
        }
    }
    pub fn row_range(&self) -> Range<usize> {
        let first = (self.scroll_offset / ROW_HEIGHT) as usize;
        let end = ((self.scroll_offset as f64 + VIEWPORT_HEIGHT as f64) / ROW_HEIGHT as f64).ceil()
            as usize;
        first.saturating_sub(2)..end.saturating_add(2).min(ROW_COUNT)
    }
    pub fn visible_text(&self) -> String {
        self.text.as_bytes()[self.text.len().saturating_sub(735)..]
            .chunks(105)
            .take(7)
            .map(|s| std::str::from_utf8(s).unwrap())
            .collect::<Vec<_>>()
            .join("\n")
    }
}
pub fn row_label(index: usize) -> String {
    format!("Row {index:06} — retained virtual item")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visible_range_uses_two_rows_of_overscan_on_each_side() {
        let mut work = Workload {
            text: String::new(),
            scroll_offset: 0.,
            frames: 0,
            mode: Mode::Idle,
            seconds: 1.,
        };
        assert_eq!(work.row_range(), 0..17);
        work.scroll_offset = 280.;
        assert_eq!(work.row_range(), 8..27);
        work.scroll_offset = ROW_COUNT as f32 * ROW_HEIGHT - VIEWPORT_HEIGHT;
        assert_eq!(work.row_range().end, ROW_COUNT);
        assert!(work.row_range().len() <= 19);
    }
    #[test]
    fn bounded_and_deterministic() {
        let mut w = Workload::from_env();
        w.mode = Mode::Both;
        for _ in 0..10_000 {
            let before = w.visible_text();
            w.tick();
            assert!(w.text.len() <= 8192);
            assert!(w.row_range().len() <= 19);
            assert_ne!(before, w.visible_text());
        }
        assert_eq!(w.scroll_offset, 140_000.);
    }
}

/// A continuously animated scene for comparing per-frame cost at the display
/// refresh rate: every dot's opacity and every shimmer character's colour
/// change on every frame, like loaders and skeleton shimmers.
pub mod animation {
    pub const COLUMNS: usize = 12;
    pub const ROWS: usize = 4;
    pub const DOT: f32 = 14.;
    pub const GAP: f32 = 12.;
    pub const ORIGIN: (f32, f32) = (40., 120.);
    pub const SHIMMER: &str = "Reading workspace files and streaming tokens";
    pub const SHIMMER_ORIGIN: (f32, f32) = (40., 260.);
    pub const DOT_COLOR: u32 = 0x9ad2ff;
    pub const BACKGROUND: u32 = 0x10141c;

    pub fn dot_position(index: usize) -> (f32, f32) {
        let (column, row) = (index % COLUMNS, index / COLUMNS);
        (
            ORIGIN.0 + column as f32 * (DOT + GAP),
            ORIGIN.1 + row as f32 * (DOT + GAP),
        )
    }
    /// A 750 ms blink with a per-dot phase offset.
    pub fn dot_opacity(index: usize, seconds: f32) -> f32 {
        let phase = (seconds / 0.75 - index as f32 * 0.07).rem_euclid(1.);
        if phase < 0.45 {
            1. - 0.9 * (1. - (1. - phase / 0.45).powi(3))
        } else if phase < 0.92 {
            0.1
        } else {
            1.
        }
    }
    /// White text whose alpha follows a light band sweeping every 3.4 s.
    pub fn shimmer_alpha(index: usize, count: usize, seconds: f32) -> u8 {
        let last = (count.max(2) - 1) as f32;
        let centre = -0.4 + 1.8 * (seconds / 3.4).rem_euclid(1.);
        let x = (1. - (index as f32 / last - centre).abs() / 0.36).clamp(0., 1.);
        (90. + 165. * x * x * (3. - 2. * x)) as u8
    }
}

/// A busy, mostly static window with one small continuous animation: the
/// case damage-based rendering is for. Only the spinner changes each frame.
pub mod busy {
    pub const GRID_COLUMNS: usize = 8;
    pub const GRID_ROWS: usize = 30;
    pub const CELL: (f32, f32) = (112., 16.);
    pub const GRID_ORIGIN: (f32, f32) = (12., 56.);
    pub const PARAGRAPH_ORIGIN: (f32, f32) = (12., 548.);
    pub const PARAGRAPH_LINES: usize = 20;
    pub const LINE_HEIGHT: f32 = 8.4;
    pub const SPINNER_CENTRE: (f32, f32) = (920., 28.);
    pub const SPINNER_DOTS: usize = 12;
    pub const SPINNER_RADIUS: f32 = 14.;
    pub const DOT: f32 = 5.;

    pub fn cell_position(index: usize) -> (f32, f32) {
        let (column, row) = (index % GRID_COLUMNS, index / GRID_COLUMNS);
        (
            GRID_ORIGIN.0 + column as f32 * (CELL.0 + 5.),
            GRID_ORIGIN.1 + row as f32 * CELL.1,
        )
    }
    pub fn cell_label(index: usize) -> String {
        format!("row {:02} · item {:03}", index / GRID_COLUMNS, index)
    }
    pub fn cell_color(index: usize) -> u32 {
        if (index / GRID_COLUMNS + index % GRID_COLUMNS).is_multiple_of(2) {
            0x1b2330
        } else {
            0x202a3a
        }
    }
    pub fn paragraph_line(index: usize) -> String {
        format!(
            "{index:02}  The quick brown fox jumps over the lazy dog while the renderer keeps every static glyph retained."
        )
    }
    pub fn dot_position(index: usize) -> (f32, f32) {
        let angle = index as f32 / SPINNER_DOTS as f32 * std::f32::consts::TAU;
        (
            SPINNER_CENTRE.0 + SPINNER_RADIUS * angle.cos() - DOT / 2.,
            SPINNER_CENTRE.1 + SPINNER_RADIUS * angle.sin() - DOT / 2.,
        )
    }
    /// A light chasing around the ring once every 0.9 s.
    pub fn dot_opacity(index: usize, seconds: f32) -> f32 {
        let head = (seconds / 0.9).rem_euclid(1.) * SPINNER_DOTS as f32;
        let behind = (head - index as f32).rem_euclid(SPINNER_DOTS as f32);
        (1. - behind / SPINNER_DOTS as f32).powi(2).max(0.12)
    }
}
