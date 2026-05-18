use std::{sync::Arc, thread, time::Duration};

use crossbeam_channel::{Receiver, TrySendError, bounded};

use crate::{process::Snapshot, source::ProcessSource};

pub fn spawn(mut source: Box<dyn ProcessSource>, interval: Duration) -> Receiver<Arc<Snapshot>> {
    let (tx, rx) = bounded::<Arc<Snapshot>>(1);

    thread::Builder::new()
        .name("rtop-sampler".to_string())
        .spawn(move || {
            let mut prev: Option<Arc<Snapshot>> = None;
            loop {
                match source.snapshot() {
                    Ok(mut snap) => {
                        if let Some(p) = prev.as_deref() {
                            fill_cpu_pct(&mut snap, p);
                        }
                        let arc = Arc::new(snap);
                        prev = Some(arc.clone());
                        // drop-NEWEST on overflow: crossbeam-channel's bounded sender can't
                        // evict the held value, so we discard the new one. With a multi-second
                        // tick and an event-driven UI, starvation isn't realistic.
                        // TODO: switch to drop-OLDEST when feasible.
                        match tx.try_send(arc) {
                            Ok(()) => {}
                            Err(TrySendError::Full(_)) => {}
                            Err(TrySendError::Disconnected(_)) => break,
                        }
                    }
                    Err(e) => {
                        eprintln!("sampler: snapshot failed: {e:#}");
                    }
                }
                thread::sleep(interval);
            }
        })
        .expect("failed to spawn sampler thread");

    rx
}

pub(crate) fn fill_cpu_pct(cur: &mut Snapshot, prev: &Snapshot) {
    let dt = match cur.sampled_at.checked_duration_since(prev.sampled_at) {
        Some(d) if !d.is_zero() => d,
        _ => return,
    };
    let dt_secs = dt.as_secs_f64();
    let n_cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1) as f64;
    let max_pct = 100.0 * n_cpus;

    for p in cur.processes.iter_mut() {
        let Some(&prev_idx) = prev.by_id.get(&p.id) else {
            continue;
        };
        let prev_proc = &prev.processes[prev_idx];
        let delta = match p.cpu_time_total.checked_sub(prev_proc.cpu_time_total) {
            Some(d) => d.as_secs_f64(),
            None => continue,
        };
        let pct = (delta / dt_secs) * 100.0;
        let clamped = pct.clamp(0.0, max_pct) as f32;
        p.cpu_pct = Some(clamped);
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, time::Instant};

    use super::*;
    use crate::process::{Process, ProcessId, Snapshot, SystemStats};

    fn mk_proc(pid: i32, start_time: u64, cpu_secs: f64) -> Process {
        Process {
            id: ProcessId { pid, start_time },
            ppid: 0,
            uid: 0,
            user: "root".into(),
            name: "x".into(),
            cmdline: vec![],
            state: 'S',
            rss_bytes: 0,
            cpu_pct: None,
            cpu_time_total: Duration::from_secs_f64(cpu_secs),
            age: Duration::ZERO,
            is_kernel_thread: false,
        }
    }

    fn mk_snap(t: Instant, procs: Vec<Process>) -> Snapshot {
        let mut by_id = HashMap::new();
        for (i, p) in procs.iter().enumerate() {
            by_id.insert(p.id, i);
        }
        Snapshot {
            processes: procs,
            by_id,
            sampled_at: t,
            system: SystemStats {
                load_1: 0.0,
                load_5: 0.0,
                load_15: 0.0,
                mem_total_bytes: 0,
                mem_used_bytes: 0,
            },
        }
    }

    #[test]
    fn normal_delta() {
        let t0 = Instant::now();
        let prev = mk_snap(t0, vec![mk_proc(100, 42, 0.0)]);
        let mut cur = mk_snap(t0 + Duration::from_secs(1), vec![mk_proc(100, 42, 0.5)]);
        fill_cpu_pct(&mut cur, &prev);
        let pct = cur.processes[0].cpu_pct.expect("cpu_pct set");
        assert!((pct - 50.0).abs() < 0.001, "expected ~50.0, got {pct}");
    }

    #[test]
    fn pid_reuse_no_match() {
        let t0 = Instant::now();
        let prev = mk_snap(t0, vec![mk_proc(100, 42, 0.0)]);
        let mut cur = mk_snap(t0 + Duration::from_secs(1), vec![mk_proc(100, 99, 0.5)]);
        fill_cpu_pct(&mut cur, &prev);
        assert!(cur.processes[0].cpu_pct.is_none());
    }

    #[test]
    fn new_process_none() {
        let t0 = Instant::now();
        let prev = mk_snap(t0, vec![]);
        let mut cur = mk_snap(t0 + Duration::from_secs(1), vec![mk_proc(100, 42, 0.5)]);
        fill_cpu_pct(&mut cur, &prev);
        assert!(cur.processes[0].cpu_pct.is_none());
    }

    #[test]
    fn zero_dt_none() {
        let t0 = Instant::now();
        let prev = mk_snap(t0, vec![mk_proc(100, 42, 0.0)]);
        let mut cur = mk_snap(t0, vec![mk_proc(100, 42, 0.5)]);
        fill_cpu_pct(&mut cur, &prev);
        assert!(cur.processes[0].cpu_pct.is_none());
    }
}
