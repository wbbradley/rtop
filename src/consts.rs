// All constants here are populated for the full app surface. Some are unused
// in Phase 1 and will land as later phases reference them.
#![allow(dead_code)]

use std::time::Duration;

use ratatui::style::Color;

pub const SAMPLE_INTERVAL: Duration = Duration::from_secs(5);
pub const SCROLLOFF: usize = 3;
pub const TREE_HALF_PAGE: usize = 10;
pub const SEARCH_BOX_HEIGHT: u16 = 3;
pub const STATUS_LINE_HEIGHT: u16 = 1;
pub const HELP_MODAL_WIDTH: u16 = 60;
pub const HELP_MODAL_HEIGHT: u16 = 20;
pub const SIGNAL_MODAL_WIDTH: u16 = 44;
pub const SIGNAL_MODAL_HEIGHT: u16 = 14;
pub const MIN_COLS: u16 = 80;
pub const MIN_ROWS: u16 = 24;
pub const ERROR_FLASH_DURATION: Duration = Duration::from_secs(3);
pub const CPU_WARN_PCT: f32 = 50.0;
pub const CPU_DANGER_PCT: f32 = 80.0;
pub const EVENT_CHANNEL_CAP: usize = 64;
pub const MACOS_ARGMAX_FALLBACK: usize = 256 * 1024;
pub const KERNEL_THREAD_PARENT_PID: i32 = 2;
pub const FOCUS_ACCENT: Color = Color::Rgb(254, 128, 25);
