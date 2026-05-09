use std::{
    collections::HashMap,
    mem,
    time::{Duration, Instant, SystemTime},
};

use anyhow::{Context, anyhow};
use libc::{
    CTL_HW,
    CTL_KERN,
    HW_MEMSIZE,
    KERN_ARGMAX,
    KERN_PROCARGS2,
    c_char,
    c_int,
    c_void,
    sysctl,
};
use libproc::{
    libproc::{
        bsd_info::BSDInfo,
        pid_rusage::{RUsageInfoV2, pidrusage},
        proc_pid::pidinfo,
    },
    processes::{ProcFilter, pids_by_type},
};
use mach2::{
    kern_return::{KERN_SUCCESS, kern_return_t},
    mach_init::mach_host_self,
    message::mach_msg_type_number_t,
    vm_statistics::vm_statistics64,
    vm_types::integer_t,
};

use crate::{
    process::{Process, ProcessId, Snapshot, SystemStats},
    source::ProcessSource,
};

// host_statistics64() is not exposed by mach2 0.6, so declare it ourselves.
const HOST_VM_INFO64: c_int = 4;

unsafe extern "C" {
    fn host_statistics64(
        host: u32,
        flavor: c_int,
        host_info_out: *mut integer_t,
        host_info_outCnt: *mut mach_msg_type_number_t,
    ) -> kern_return_t;
}

pub struct MacOsProcessSource {
    user_cache: HashMap<u32, String>,
    page_size: u64,
    argmax: usize,
    args_buf: Vec<u8>,
}

impl MacOsProcessSource {
    pub fn new() -> anyhow::Result<Self> {
        // Probe pidrusage(getpid()) to surface unreadability before raw mode,
        // mirroring the Linux source's /proc/self/stat probe.
        let my_pid = unsafe { libc::getpid() };
        let _: RUsageInfoV2 = pidrusage(my_pid)
            .map_err(|e| anyhow!("{e}"))
            .context("failed to read self pidrusage")?;

        let argmax = read_argmax().unwrap_or(256 * 1024);
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as u64;

        Ok(Self {
            user_cache: HashMap::new(),
            page_size,
            argmax,
            args_buf: Vec::with_capacity(argmax),
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

impl ProcessSource for MacOsProcessSource {
    fn snapshot(&mut self) -> anyhow::Result<Snapshot> {
        let sampled_at = Instant::now();
        let now_wall = SystemTime::now();

        let pids = pids_by_type(ProcFilter::All).context("failed to list process ids")?;

        let mut processes: Vec<Process> = Vec::with_capacity(pids.len());
        for pid_u in pids {
            // pid 0 is the kernel_task surrogate; pidinfo/pidrusage reject it.
            if pid_u == 0 {
                continue;
            }
            let pid = pid_u as i32;

            let bsd: BSDInfo = match pidinfo(pid, 0) {
                Ok(b) => b,
                Err(_) => continue,
            };
            let rusage: RUsageInfoV2 = match pidrusage(pid) {
                Ok(r) => r,
                Err(_) => continue,
            };

            let ppid = bsd.pbi_ppid as i32;
            let uid = bsd.pbi_uid;
            let state = map_state(bsd.pbi_status);

            let start_secs = bsd.pbi_start_tvsec;
            let start_usecs = bsd.pbi_start_tvusec;
            let start_time = start_secs
                .saturating_mul(1_000_000)
                .saturating_add(start_usecs);

            let proc_start = SystemTime::UNIX_EPOCH
                + Duration::new(start_secs, (start_usecs as u32).saturating_mul(1000));
            let age = now_wall
                .duration_since(proc_start)
                .unwrap_or(Duration::ZERO);

            let cpu_time_total =
                Duration::from_nanos(rusage.ri_user_time.saturating_add(rusage.ri_system_time));
            let rss_bytes = rusage.ri_resident_size;

            let cmdline = fetch_procargs2(pid, &mut self.args_buf, self.argmax);
            let name = comm_to_string(&bsd.pbi_comm);
            let user = self.resolve_user(uid);

            let id = ProcessId { pid, start_time };

            processes.push(Process {
                id,
                ppid,
                uid,
                user,
                name,
                cmdline,
                state,
                rss_bytes,
                cpu_pct: None,
                cpu_time_total,
                age,
                is_kernel_thread: false,
            });
        }

        let mut by_id = HashMap::with_capacity(processes.len());
        for (i, p) in processes.iter().enumerate() {
            by_id.insert(p.id, i);
        }

        let load = read_load_avg();
        let (mem_total, mem_used) = read_mem(self.page_size);

        Ok(Snapshot {
            processes,
            by_id,
            sampled_at,
            system: SystemStats {
                load_1: load[0],
                load_5: load[1],
                load_15: load[2],
                mem_total_bytes: mem_total,
                mem_used_bytes: mem_used,
            },
        })
    }
}

fn map_state(pbi_status: u32) -> char {
    // sys/proc.h state constants.
    match pbi_status {
        1 => 'I', // SIDL
        2 => 'R', // SRUN
        3 => 'S', // SSLEEP
        4 => 'T', // SSTOP
        5 => 'Z', // SZOMB
        _ => '?',
    }
}

fn comm_to_string(comm: &[c_char]) -> String {
    let end = comm.iter().position(|&c| c == 0).unwrap_or(comm.len());
    let bytes: Vec<u8> = comm[..end].iter().map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn read_argmax() -> Option<usize> {
    let mut mib = [CTL_KERN, KERN_ARGMAX];
    let mut value: c_int = 0;
    let mut size = mem::size_of::<c_int>();
    let res = unsafe {
        sysctl(
            mib.as_mut_ptr(),
            mib.len() as u32,
            &mut value as *mut c_int as *mut c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if res < 0 || value <= 0 {
        return None;
    }
    Some(value as usize)
}

fn fetch_procargs2(pid: i32, buf: &mut Vec<u8>, argmax: usize) -> Vec<String> {
    if argmax == 0 {
        return Vec::new();
    }
    buf.clear();
    buf.resize(argmax, 0u8);
    let mut mib = [CTL_KERN, KERN_PROCARGS2, pid];
    let mut size = buf.len();
    let res = unsafe {
        sysctl(
            mib.as_mut_ptr(),
            mib.len() as u32,
            buf.as_mut_ptr() as *mut c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if res < 0 {
        return Vec::new();
    }
    parse_procargs2(&buf[..size])
}

// KERN_PROCARGS2 buffer layout:
//   [argc:i32 native-endian][exec_path \0][\0 padding...]
//   [argv[0] \0][argv[1] \0]...[argv[argc-1] \0][env...]
fn parse_procargs2(buf: &[u8]) -> Vec<String> {
    if buf.len() < 4 {
        return Vec::new();
    }
    let argc = i32::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if argc <= 0 {
        return Vec::new();
    }
    let mut i: usize = 4;
    while i < buf.len() && buf[i] != 0 {
        i += 1;
    }
    while i < buf.len() && buf[i] == 0 {
        i += 1;
    }
    let mut out = Vec::with_capacity(argc as usize);
    for _ in 0..argc {
        if i >= buf.len() {
            break;
        }
        let start = i;
        while i < buf.len() && buf[i] != 0 {
            i += 1;
        }
        out.push(String::from_utf8_lossy(&buf[start..i]).into_owned());
        if i < buf.len() {
            i += 1;
        }
    }
    out
}

fn read_load_avg() -> [f32; 3] {
    let mut loads = [0.0f64; 3];
    let n = unsafe { libc::getloadavg(loads.as_mut_ptr(), 3) };
    if n != 3 {
        return [0.0; 3];
    }
    [loads[0] as f32, loads[1] as f32, loads[2] as f32]
}

fn read_mem(page_size: u64) -> (u64, u64) {
    let mut mib = [CTL_HW, HW_MEMSIZE];
    let mut total: u64 = 0;
    let mut size = mem::size_of::<u64>();
    let res = unsafe {
        sysctl(
            mib.as_mut_ptr(),
            mib.len() as u32,
            &mut total as *mut u64 as *mut c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if res < 0 {
        return (0, 0);
    }

    let mut vm: vm_statistics64 = unsafe { mem::zeroed() };
    let mut count: mach_msg_type_number_t =
        (mem::size_of::<vm_statistics64>() / mem::size_of::<integer_t>()) as mach_msg_type_number_t;
    let kr = unsafe {
        host_statistics64(
            mach_host_self(),
            HOST_VM_INFO64,
            &mut vm as *mut _ as *mut integer_t,
            &mut count,
        )
    };
    if kr != KERN_SUCCESS {
        return (total, 0);
    }
    // Activity Monitor's "memory used" ≈ active + wired + compressed.
    let active = vm.active_count as u64;
    let wired = vm.wire_count as u64;
    let compressed = vm.compressor_page_count as u64;
    let used_pages = active.saturating_add(wired).saturating_add(compressed);
    let used = used_pages.saturating_mul(page_size);
    (total, used)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke() {
        let mut src = MacOsProcessSource::new().expect("source ctor");
        let snap = src.snapshot().expect("snapshot");
        assert!(!snap.processes.is_empty());
        let me = std::process::id() as i32;
        assert!(
            snap.processes.iter().any(|p| p.id.pid == me),
            "current pid {me} not present in snapshot"
        );
    }

    #[test]
    fn map_state_known_values() {
        assert_eq!(map_state(1), 'I');
        assert_eq!(map_state(2), 'R');
        assert_eq!(map_state(3), 'S');
        assert_eq!(map_state(4), 'T');
        assert_eq!(map_state(5), 'Z');
        assert_eq!(map_state(0), '?');
        assert_eq!(map_state(99), '?');
    }

    #[test]
    fn parse_procargs2_basic() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&2i32.to_ne_bytes());
        buf.extend_from_slice(b"/usr/bin/foo\0");
        buf.extend_from_slice(b"\0\0");
        buf.extend_from_slice(b"foo\0");
        buf.extend_from_slice(b"bar\0");
        buf.extend_from_slice(b"PATH=/bin\0");

        let args = parse_procargs2(&buf);
        assert_eq!(args, vec!["foo".to_string(), "bar".to_string()]);
    }

    #[test]
    fn parse_procargs2_empty_on_short_buffer() {
        assert!(parse_procargs2(&[]).is_empty());
        assert!(parse_procargs2(&[0, 0]).is_empty());
    }

    #[test]
    fn parse_procargs2_zero_argc() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&0i32.to_ne_bytes());
        buf.extend_from_slice(b"/usr/bin/foo\0\0");
        assert!(parse_procargs2(&buf).is_empty());
    }
}
