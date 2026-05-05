// All constants here are populated for the full app surface. Some are unused
// in Phase 1 and will land as later phases reference them.
#![allow(dead_code)]

use std::time::Duration;

pub const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);
pub const LOAD_VIEW_VISIBLE_ROWS: usize = 10;
pub const SCROLLOFF: usize = 3;
pub const MIN_COLS: u16 = 80;
pub const MIN_ROWS: u16 = 24;
pub const ERROR_FLASH_DURATION: Duration = Duration::from_secs(3);
pub const CPU_WARN_PCT: f32 = 50.0;
pub const CPU_DANGER_PCT: f32 = 80.0;
