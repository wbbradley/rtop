//! Best-effort persistence of the interactive session across runs.
//!
//! This stores runtime *state* (the search query and view flags), which is
//! distinct from a user-editable *config* file (still out of scope). Every
//! function here is best-effort: it never panics and never aborts startup or
//! shutdown. A missing file yields defaults; a corrupt or unreadable file is
//! ignored. All path/IO is intended to run outside raw mode so a state-file
//! failure can never leave the terminal in a bad state.
//!
//! This module is the serde boundary: the domain types (`ProcessId`, `Focus`)
//! stay serde-free and are converted to/from the primitive fields here.

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::{app::event::Focus, consts::STATE_FILE_NAME, process::ProcessId};

/// Serializable mirror of [`Focus`], kept separate so the domain type needs no
/// serde derives.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PersistedFocus {
    #[default]
    Search,
    Tree,
}

impl From<Focus> for PersistedFocus {
    fn from(f: Focus) -> Self {
        match f {
            Focus::Search => PersistedFocus::Search,
            Focus::Tree => PersistedFocus::Tree,
        }
    }
}

impl From<PersistedFocus> for Focus {
    fn from(f: PersistedFocus) -> Self {
        match f {
            PersistedFocus::Search => Focus::Search,
            PersistedFocus::Tree => Focus::Tree,
        }
    }
}

/// The full persisted session — only user-facing fields meaningful across a
/// restart. Derived/ephemeral state (matched pids, compiled query, tree cache,
/// modals, flashes) is intentionally excluded and re-derived on boot.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PersistedState {
    /// Canonical search query; `query`/`compiled` are re-derived from it.
    pub query_text: String,
    /// Caret position (codepoint index) within `query_text`. `None` in older
    /// state files that predate cursor persistence; those restore with the
    /// caret at the end of the query. Kept as a plain `Option<usize>` so the
    /// persisted schema stays explicit (no `tui-input` serde feature).
    pub query_cursor: Option<usize>,
    pub focus: PersistedFocus,
    pub paused: bool,
    pub hide_kernel_threads: bool,
    /// Tree-cursor anchor, stored as primitives. Both are set together; a
    /// re-anchor by full `ProcessId` (pid + start_time) survives PID reuse.
    pub cursor_pid: Option<i32>,
    pub cursor_start_time: Option<u64>,
}

impl PersistedState {
    /// The restored cursor anchor as a `ProcessId`, if both primitives are set.
    pub fn cursor_id(&self) -> Option<ProcessId> {
        match (self.cursor_pid, self.cursor_start_time) {
            (Some(pid), Some(start_time)) => Some(ProcessId { pid, start_time }),
            _ => None,
        }
    }
}

/// Resolve the effective boot state from the restored state plus CLI overrides.
///
/// Precedence: `--no-restore` discards the restored state entirely; a non-empty
/// `--filter` overrides the query; `--no-kernel-threads` forces hide = true.
/// Otherwise restored values apply.
pub fn resolve_boot(
    restored: PersistedState,
    no_restore: bool,
    filter: Option<String>,
    force_hide_kernel_threads: bool,
) -> PersistedState {
    let mut resolved = if no_restore {
        PersistedState::default()
    } else {
        restored
    };
    if let Some(f) = filter.filter(|s| !s.is_empty()) {
        resolved.query_text = f;
        // The CLI-provided filter is a fresh query; drop any restored caret so
        // it lands at the end of the new text.
        resolved.query_cursor = None;
    }
    if force_hide_kernel_threads {
        resolved.hide_kernel_threads = true;
    }
    resolved
}

/// Location of the state file: `state_dir()` when the platform has one (Linux
/// `~/.local/state/rtop/`), else `data_dir()` (macOS
/// `~/Library/Application Support/rtop/`). `None` if no home dir can be found.
pub fn state_file_path() -> Option<PathBuf> {
    let dirs = ProjectDirs::from("", "", "rtop")?;
    let dir = dirs.state_dir().unwrap_or_else(|| dirs.data_dir());
    Some(dir.join(STATE_FILE_NAME))
}

/// Load the persisted state. Returns default on any error (missing / corrupt /
/// unreadable / no home dir).
pub fn load() -> PersistedState {
    match state_file_path() {
        Some(path) => load_from(&path),
        None => PersistedState::default(),
    }
}

/// Save the persisted state atomically (temp file in the same dir, then rename).
/// Best-effort: returns the IO error to the caller, which is expected to ignore
/// it. Never panics. A `None` path (no home dir) is a silent no-op.
pub fn save(state: &PersistedState) -> io::Result<()> {
    match state_file_path() {
        Some(path) => save_to_path(&path, state),
        None => Ok(()),
    }
}

fn load_from(path: &Path) -> PersistedState {
    match fs::read(path) {
        Ok(bytes) => decode(&bytes),
        Err(_) => PersistedState::default(),
    }
}

fn decode(bytes: &[u8]) -> PersistedState {
    serde_json::from_slice(bytes).unwrap_or_default()
}

fn save_to_path(path: &Path, state: &PersistedState) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec_pretty(state).map_err(io::Error::other)?;
    // Write to a sibling temp file, then rename → atomic replace within the dir.
    let mut tmp = path.to_path_buf();
    tmp.set_extension("tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(&json)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PersistedState {
        PersistedState {
            query_text: "cmd:fire user:root".to_string(),
            query_cursor: Some(7),
            focus: PersistedFocus::Tree,
            paused: true,
            hide_kernel_threads: true,
            cursor_pid: Some(4321),
            cursor_start_time: Some(999_888),
        }
    }

    #[test]
    fn round_trip_serde() {
        let s = sample();
        let json = serde_json::to_string(&s).unwrap();
        let back: PersistedState = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn decode_valid() {
        let s = sample();
        let json = serde_json::to_vec(&s).unwrap();
        assert_eq!(decode(&json), s);
    }

    #[test]
    fn decode_corrupt_returns_default() {
        assert_eq!(decode(b"{ this is not json"), PersistedState::default());
    }

    #[test]
    fn decode_empty_returns_default() {
        assert_eq!(decode(b""), PersistedState::default());
    }

    #[test]
    fn decode_partial_json_uses_field_defaults() {
        // serde(default) → missing fields fall back to defaults, not an error.
        let s = decode(br#"{"query_text":"foo"}"#);
        assert_eq!(s.query_text, "foo");
        assert_eq!(s.query_cursor, None);
        assert_eq!(s.focus, PersistedFocus::Search);
        assert!(!s.paused);
        assert_eq!(s.cursor_pid, None);
    }

    #[test]
    fn load_from_missing_returns_default() {
        let path = Path::new("/nonexistent/rtop-test/definitely/not/here.json");
        assert_eq!(load_from(path), PersistedState::default());
    }

    #[test]
    fn save_then_load_from_round_trip() {
        let mut path = std::env::temp_dir();
        path.push(format!("rtop-persist-test-{}", std::process::id()));
        path.push("state.json");
        let _ = fs::remove_dir_all(path.parent().unwrap());

        let s = sample();
        save_to_path(&path, &s).expect("save");
        assert_eq!(load_from(&path), s);

        // Overwrite (exercises rename-over-existing).
        let mut s2 = sample();
        s2.query_text = "changed".to_string();
        save_to_path(&path, &s2).expect("save again");
        assert_eq!(load_from(&path), s2);

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn cursor_id_requires_both_primitives() {
        let mut s = PersistedState::default();
        assert_eq!(s.cursor_id(), None);
        s.cursor_pid = Some(10);
        assert_eq!(s.cursor_id(), None);
        s.cursor_start_time = Some(20);
        assert_eq!(
            s.cursor_id(),
            Some(ProcessId {
                pid: 10,
                start_time: 20
            })
        );
    }

    #[test]
    fn focus_conversion_round_trips() {
        for f in [Focus::Search, Focus::Tree] {
            assert_eq!(Focus::from(PersistedFocus::from(f)), f);
        }
        for pf in [PersistedFocus::Search, PersistedFocus::Tree] {
            assert_eq!(PersistedFocus::from(Focus::from(pf)), pf);
        }
    }

    #[test]
    fn resolve_boot_no_restore_ignores_restored() {
        let restored = sample();
        let boot = resolve_boot(restored, true, None, false);
        assert_eq!(boot, PersistedState::default());
    }

    #[test]
    fn resolve_boot_nonempty_filter_overrides_query() {
        let restored = sample();
        let boot = resolve_boot(restored, false, Some("name:zsh".to_string()), false);
        assert_eq!(boot.query_text, "name:zsh");
        // The CLI filter resets the caret so it lands at end of the new text.
        assert_eq!(boot.query_cursor, None);
        // Other restored fields survive.
        assert_eq!(boot.focus, PersistedFocus::Tree);
        assert!(boot.paused);
    }

    #[test]
    fn resolve_boot_absent_filter_keeps_restored_query() {
        let restored = sample();
        let boot = resolve_boot(restored.clone(), false, None, false);
        assert_eq!(boot.query_text, restored.query_text);
    }

    #[test]
    fn resolve_boot_empty_filter_keeps_restored_query() {
        let restored = sample();
        let boot = resolve_boot(restored.clone(), false, Some(String::new()), false);
        assert_eq!(boot.query_text, restored.query_text);
    }

    #[test]
    fn resolve_boot_force_hide_forces_true() {
        let mut restored = sample();
        restored.hide_kernel_threads = false;
        let boot = resolve_boot(restored, false, None, true);
        assert!(boot.hide_kernel_threads);
    }

    #[test]
    fn resolve_boot_restored_hide_preserved_without_force() {
        let mut restored = sample();
        restored.hide_kernel_threads = true;
        let boot = resolve_boot(restored, false, None, false);
        assert!(boot.hide_kernel_threads);
    }

    #[test]
    fn resolve_boot_no_restore_with_filter_and_force() {
        // --no-restore starts fresh but still honors CLI query/toggles.
        let boot = resolve_boot(sample(), true, Some("pid:1".to_string()), true);
        assert_eq!(boot.query_text, "pid:1");
        assert_eq!(boot.query_cursor, None);
        assert!(boot.hide_kernel_threads);
        assert_eq!(boot.focus, PersistedFocus::Search);
        assert!(!boot.paused);
        assert_eq!(boot.cursor_pid, None);
    }
}
