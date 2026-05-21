use crate::{
    process::Process,
    search::parser::{Field, Query, Term},
};

/// Per-process predicate. An empty query matches everything; otherwise the
/// process must satisfy at least one OR-group (all terms in that group).
pub fn matches(query: &Query, p: &Process) -> bool {
    if query.groups.is_empty() {
        return true;
    }
    query
        .groups
        .iter()
        .any(|group| group.iter().all(|term| term_matches(p, term)))
}

fn term_matches(p: &Process, term: &Term) -> bool {
    match term {
        Term::Prefixed { field, value } => match field {
            Field::Pid => match value.parse::<i32>() {
                Ok(n) => p.id.pid == n,
                Err(_) => false,
            },
            Field::Ppid => match value.parse::<i32>() {
                Ok(n) => p.ppid == n,
                Err(_) => false,
            },
            Field::User => contains_ci(&p.user, value),
            Field::Name => contains_ci(&p.name, value),
            Field::Cmd => contains_ci(&cmdline_joined(p), value),
            Field::State => p.state.eq_ignore_ascii_case(&first_char(value)),
        },
        Term::Bare(s) => {
            let haystack = format!("{} {} {}", p.name, cmdline_joined(p), p.user);
            contains_ci(&haystack, s)
        }
    }
}

fn contains_ci(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

fn cmdline_joined(p: &Process) -> String {
    p.cmdline.join(" ")
}

fn first_char(s: &str) -> char {
    s.chars().next().unwrap_or('\0')
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        time::{Duration, Instant},
    };

    use super::*;
    use crate::process::{Process, ProcessId, Snapshot, SystemStats};

    fn mk_proc(
        pid: i32,
        ppid: i32,
        user: &str,
        name: &str,
        cmdline: &[&str],
        state: char,
    ) -> Process {
        Process {
            id: ProcessId {
                pid,
                start_time: pid as u64,
            },
            ppid,
            uid: 0,
            user: user.into(),
            name: name.into(),
            cmdline: cmdline.iter().map(|s| s.to_string()).collect(),
            state,
            rss_bytes: 0,
            cpu_pct: Some(1.0),
            cpu_time_total: Duration::ZERO,
            age: Duration::ZERO,
            is_kernel_thread: false,
        }
    }

    fn fixture() -> Snapshot {
        let procs = vec![
            mk_proc(1, 0, "root", "systemd", &["/sbin/init"], 'S'),
            mk_proc(42, 1, "root", "sshd", &["sshd: listener"], 'S'),
            mk_proc(101, 1, "wbbradley", "bash", &["-bash"], 'S'),
            mk_proc(
                202,
                101,
                "wbbradley",
                "firefox",
                &["/usr/bin/firefox", "--profile"],
                'R',
            ),
            mk_proc(303, 1, "wbbradley", "vim", &["vim", "src/main.rs"], 'S'),
        ];
        let mut by_id = HashMap::new();
        for (i, p) in procs.iter().enumerate() {
            by_id.insert(p.id, i);
        }
        Snapshot {
            processes: procs,
            by_id,
            sampled_at: Instant::now(),
            system: SystemStats {
                load_1: 0.0,
                load_5: 0.0,
                load_15: 0.0,
                mem_total_bytes: 0,
                mem_used_bytes: 0,
            },
        }
    }

    fn pid_of(snap: &Snapshot, pid: i32) -> &Process {
        snap.processes.iter().find(|p| p.id.pid == pid).unwrap()
    }

    #[test]
    fn empty_query_matches_all() {
        let snap = fixture();
        let q = crate::search::parse("");
        for p in &snap.processes {
            assert!(matches(&q, p));
        }
    }

    #[test]
    fn matches_pid_exact() {
        let snap = fixture();
        let q = crate::search::parse("pid:42");
        assert!(matches(&q, pid_of(&snap, 42)));
        assert!(!matches(&q, pid_of(&snap, 101)));
    }

    #[test]
    fn matches_ppid() {
        let snap = fixture();
        let q = crate::search::parse("ppid:1");
        assert!(matches(&q, pid_of(&snap, 42)));
        assert!(matches(&q, pid_of(&snap, 101)));
        assert!(matches(&q, pid_of(&snap, 303)));
        assert!(!matches(&q, pid_of(&snap, 1)));
        assert!(!matches(&q, pid_of(&snap, 202)));
    }

    #[test]
    fn matches_user_case_insensitive() {
        let snap = fixture();
        let q = crate::search::parse("user:WBB");
        assert!(matches(&q, pid_of(&snap, 101)));
        assert!(!matches(&q, pid_of(&snap, 1)));
    }

    #[test]
    fn matches_name() {
        let snap = fixture();
        let q = crate::search::parse("name:vim");
        assert!(matches(&q, pid_of(&snap, 303)));
        assert!(!matches(&q, pid_of(&snap, 202)));
    }

    #[test]
    fn matches_cmd() {
        let snap = fixture();
        let q = crate::search::parse("cmd:profile");
        assert!(matches(&q, pid_of(&snap, 202)));
        assert!(!matches(&q, pid_of(&snap, 303)));
    }

    #[test]
    fn matches_state() {
        let snap = fixture();
        let q = crate::search::parse("state:R");
        assert!(matches(&q, pid_of(&snap, 202)));
        assert!(!matches(&q, pid_of(&snap, 101)));
    }

    #[test]
    fn matches_bare_substring() {
        let snap = fixture();
        let q = crate::search::parse("firef");
        assert!(matches(&q, pid_of(&snap, 202)));
        assert!(!matches(&q, pid_of(&snap, 303)));
    }

    #[test]
    fn matches_and_within_group() {
        let snap = fixture();
        let q = crate::search::parse("user:wbbradley name:vim");
        assert!(matches(&q, pid_of(&snap, 303)));
        assert!(!matches(&q, pid_of(&snap, 202)));
        assert!(!matches(&q, pid_of(&snap, 1)));
    }

    #[test]
    fn matches_or_across_groups() {
        let snap = fixture();
        let q = crate::search::parse("name:firefox, name:vim");
        assert!(matches(&q, pid_of(&snap, 202)));
        assert!(matches(&q, pid_of(&snap, 303)));
        assert!(!matches(&q, pid_of(&snap, 1)));
        assert!(!matches(&q, pid_of(&snap, 42)));
    }

    #[test]
    fn auto_select_pid_picked_up_from_query() {
        let q = crate::search::parse("pid:42");
        assert_eq!(q.auto_select_pid, Some(42));
    }
}
