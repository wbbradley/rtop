use crate::process::Snapshot;

pub trait ProcessSource: Send {
    fn snapshot(&mut self) -> anyhow::Result<Snapshot>;
}

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "linux")]
pub use linux::LinuxProcessSource as PlatformSource;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "macos")]
pub use macos::MacOsProcessSource as PlatformSource;
