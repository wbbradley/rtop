use std::{
    collections::HashMap,
    time::{Duration, Instant, SystemTime},
};

use anyhow::{Context, anyhow};
use procfs::{Current, ProcError, process::Process as ProcProcess};

use crate::{
    process::{Process, ProcessId, Snapshot, SystemStats},
    source::ProcessSource,
};

pub struct LinuxProcessSource {
    user_cache: HashMap<u32, String>,
    ticks_per_sec: u64,
    page_size: u64,
    boot_time: SystemTime,
}

impl LinuxProcessSource {
    pub fn new() -> anyhow::Result<Self> {
        // Probe /proc/self/stat to surface unreadability immediately, before raw mode.
        ProcProcess::myself()
            .and_then(|p| p.stat())
            .context("failed to read /proc/self/stat")?;

        let ticks_per_sec = procfs::ticks_per_second();
        let page_size = procfs::page_size();
        let boot_secs = procfs::boot_time_secs().context("failed to read boot time")?;
        let boot_time = SystemTime::UNIX_EPOCH + Duration::from_secs(boot_secs);

        Ok(Self {
            user_cache: HashMap::new(),
            ticks_per_sec,
            page_size,
            boot_time,
        })
    }

    fn resolve_user(&mut self, uid: u32) -> String {
        if let Some(name) = self.user_cache.get(&uid) {
            return name.clone();
        }
        let name = uzers::get_user_by_uid(uid)
            .and_then(|u| u.name().to_str().map(String::from))
            .unwrap_or_else(|| uid.to_string());
        self.user_cache.insert(uid, name.clone());
        name
    }
}

impl ProcessSource for LinuxProcessSource {
    fn snapshot(&mut self) -> anyhow::Result<Snapshot> {
        let sampled_at = Instant::now();
        let now_wall = SystemTime::now();

        let mut processes: Vec<Process> = Vec::new();

        let iter = procfs::process::all_processes().context("failed to enumerate /proc")?;
        for proc_res in iter {
            let proc = match proc_res {
                Ok(p) => p,
                Err(ProcError::NotFound(_)) => continue,
                Err(e) => return Err(anyhow!(e).context("failed to open /proc/<pid>")),
            };

            let stat = match proc.stat() {
                Ok(s) => s,
                Err(ProcError::NotFound(_)) => continue,
                Err(e) => return Err(anyhow!(e).context("failed to read stat")),
            };
            let status = match proc.status() {
                Ok(s) => s,
                Err(ProcError::NotFound(_)) => continue,
                Err(e) => return Err(anyhow!(e).context("failed to read status")),
            };
            let cmdline = match proc.cmdline() {
                Ok(c) => c,
                Err(ProcError::NotFound(_)) => continue,
                Err(_) => Vec::new(),
            };

            let uid = status.ruid;
            let user = self.resolve_user(uid);

            let cpu_ticks = stat.utime.saturating_add(stat.stime);
            let cpu_time_total =
                Duration::from_secs_f64(cpu_ticks as f64 / self.ticks_per_sec as f64);

            let start_offset =
                Duration::from_secs_f64(stat.starttime as f64 / self.ticks_per_sec as f64);
            let proc_start = self.boot_time + start_offset;
            let age = now_wall
                .duration_since(proc_start)
                .unwrap_or(Duration::ZERO);

            let rss_bytes = stat.rss.saturating_mul(self.page_size);
            let is_kernel_thread = stat.ppid == 2 || stat.pid == 2;

            let id = ProcessId {
                pid: stat.pid,
                start_time: stat.starttime,
            };

            processes.push(Process {
                id,
                ppid: stat.ppid,
                uid,
                user,
                name: stat.comm.clone(),
                cmdline,
                state: stat.state,
                rss_bytes,
                cpu_pct: None,
                cpu_time_total,
                age,
                is_kernel_thread,
            });
        }

        let mut by_id = HashMap::with_capacity(processes.len());
        for (i, p) in processes.iter().enumerate() {
            by_id.insert(p.id, i);
        }

        let load = procfs::LoadAverage::current().context("failed to read /proc/loadavg")?;
        let mem = procfs::Meminfo::current().context("failed to read /proc/meminfo")?;
        let mem_total = mem.mem_total;
        let mem_used = match mem.mem_available {
            Some(avail) => mem_total.saturating_sub(avail),
            None => mem_total.saturating_sub(mem.mem_free),
        };

        let system = SystemStats {
            load_1: load.one,
            load_5: load.five,
            load_15: load.fifteen,
            mem_total_bytes: mem_total,
            mem_used_bytes: mem_used,
        };

        Ok(Snapshot {
            processes,
            by_id,
            sampled_at,
            system,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke() {
        let mut src = LinuxProcessSource::new().expect("source ctor");
        let snap = src.snapshot().expect("snapshot");
        assert!(!snap.processes.is_empty());
        let me = std::process::id() as i32;
        assert!(
            snap.processes.iter().any(|p| p.id.pid == me),
            "current pid {me} not present in snapshot"
        );
    }
}
