// IR fields are populated in P1 even where Phase 1's renderer doesn't read them yet
// (later phases will). Suppress dead_code for the whole module.
#![allow(dead_code)]

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProcessId {
    pub pid: i32,
    pub start_time: u64,
}

#[derive(Clone, Debug)]
pub struct Process {
    pub id: ProcessId,
    pub ppid: i32,
    pub uid: u32,
    pub user: String,
    pub name: String,
    pub cmdline: Vec<String>,
    pub state: char,
    pub rss_bytes: u64,
    pub cpu_pct: Option<f32>,
    pub cpu_time_total: Duration,
    pub age: Duration,
    pub is_kernel_thread: bool,
}

#[derive(Clone, Debug)]
pub struct SystemStats {
    pub load_1: f32,
    pub load_5: f32,
    pub load_15: f32,
    pub mem_total_bytes: u64,
    pub mem_used_bytes: u64,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub processes: Vec<Process>,
    pub by_id: HashMap<ProcessId, usize>,
    pub sampled_at: Instant,
    pub system: SystemStats,
}
