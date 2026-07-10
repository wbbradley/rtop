use crate::{
    process::Process,
    search::compiled::{CompiledQuery, CompiledTerm, StrTarget},
};

/// Per-process predicate. An empty query matches everything; otherwise the
/// process must satisfy at least one OR-group (all terms in that group).
pub fn matches(cq: &CompiledQuery, p: &Process) -> bool {
    if cq.is_empty() {
        return true;
    }
    cq.groups()
        .iter()
        .any(|group| group.iter().all(|term| term_matches(p, term)))
}

fn term_matches(p: &Process, term: &CompiledTerm) -> bool {
    match term {
        CompiledTerm::Pid(n) => p.id.pid == *n,
        CompiledTerm::Ppid(n) => p.ppid == *n,
        CompiledTerm::State(c) => p.state.eq_ignore_ascii_case(c),
        CompiledTerm::Str { field, re } => match field {
            StrTarget::User => re.is_match(&p.user),
            StrTarget::Name => re.is_match(&p.name),
            StrTarget::Cmd => re.is_match(&cmdline_joined(p)),
            StrTarget::Bare => {
                let haystack = format!("{} {} {}", p.name, cmdline_joined(p), p.user);
                re.is_match(&haystack)
            }
        },
        // Uncompilable regex is non-constraining within its AND-group.
        CompiledTerm::Invalid => true,
        // e.g. `ppid:<non-int>` never matches.
        CompiledTerm::Never => false,
    }
}

fn cmdline_joined(p: &Process) -> String {
    p.cmdline.join(" ")
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

    fn compiled(s: &str) -> CompiledQuery {
        CompiledQuery::compile(&crate::search::parse(s))
    }

    #[test]
    fn empty_query_matches_all() {
        let snap = fixture();
        let q = compiled("");
        for p in &snap.processes {
            assert!(matches(&q, p));
        }
    }

    #[test]
    fn matches_pid_exact() {
        let snap = fixture();
        let q = compiled("pid:42");
        assert!(matches(&q, pid_of(&snap, 42)));
        assert!(!matches(&q, pid_of(&snap, 101)));
    }

    #[test]
    fn matches_ppid() {
        let snap = fixture();
        let q = compiled("ppid:1");
        assert!(matches(&q, pid_of(&snap, 42)));
        assert!(matches(&q, pid_of(&snap, 101)));
        assert!(matches(&q, pid_of(&snap, 303)));
        assert!(!matches(&q, pid_of(&snap, 1)));
        assert!(!matches(&q, pid_of(&snap, 202)));
    }

    #[test]
    fn matches_user_case_insensitive() {
        let snap = fixture();
        let q = compiled("user:WBB");
        assert!(matches(&q, pid_of(&snap, 101)));
        assert!(!matches(&q, pid_of(&snap, 1)));
    }

    #[test]
    fn matches_name() {
        let snap = fixture();
        let q = compiled("name:vim");
        assert!(matches(&q, pid_of(&snap, 303)));
        assert!(!matches(&q, pid_of(&snap, 202)));
    }

    #[test]
    fn matches_cmd() {
        let snap = fixture();
        let q = compiled("cmd:profile");
        assert!(matches(&q, pid_of(&snap, 202)));
        assert!(!matches(&q, pid_of(&snap, 303)));
    }

    #[test]
    fn matches_state() {
        let snap = fixture();
        let q = compiled("state:R");
        assert!(matches(&q, pid_of(&snap, 202)));
        assert!(!matches(&q, pid_of(&snap, 101)));
    }

    #[test]
    fn matches_bare_substring() {
        let snap = fixture();
        let q = compiled("firef");
        assert!(matches(&q, pid_of(&snap, 202)));
        assert!(!matches(&q, pid_of(&snap, 303)));
    }

    #[test]
    fn matches_and_within_group() {
        let snap = fixture();
        let q = compiled("user:wbbradley name:vim");
        assert!(matches(&q, pid_of(&snap, 303)));
        assert!(!matches(&q, pid_of(&snap, 202)));
        assert!(!matches(&q, pid_of(&snap, 1)));
    }

    #[test]
    fn matches_or_across_groups() {
        let snap = fixture();
        let q = compiled("name:firefox, name:vim");
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

    #[test]
    fn matches_or_across_groups_no_whitespace() {
        let snap = fixture();
        let q = compiled("name:firefox,name:vim");
        assert!(matches(&q, pid_of(&snap, 202)));
        assert!(matches(&q, pid_of(&snap, 303)));
        assert!(!matches(&q, pid_of(&snap, 1)));
    }

    // ---- regex semantics ----

    #[test]
    fn regex_anchor_start_on_name() {
        let snap = fixture();
        let q = compiled("name:^fire");
        assert!(matches(&q, pid_of(&snap, 202)));
        assert!(!matches(&q, pid_of(&snap, 42)));
    }

    #[test]
    fn regex_anchor_end_on_cmd() {
        let snap = fixture();
        // firefox cmdline joins to "/usr/bin/firefox --profile".
        let q = compiled("cmd:profile$");
        assert!(matches(&q, pid_of(&snap, 202)));
        assert!(!matches(&q, pid_of(&snap, 303)));
    }

    #[test]
    fn regex_full_anchor_on_user() {
        let snap = fixture();
        let q = compiled("user:^root$");
        assert!(matches(&q, pid_of(&snap, 42)));
        assert!(!matches(&q, pid_of(&snap, 101)));
    }

    #[test]
    fn regex_bare_wildcard() {
        let snap = fixture();
        let q = compiled("fire.*fox");
        assert!(matches(&q, pid_of(&snap, 202)));
        assert!(!matches(&q, pid_of(&snap, 303)));
    }

    #[test]
    fn regex_case_insensitive_by_default() {
        let snap = fixture();
        let q = compiled("name:FIRE");
        assert!(matches(&q, pid_of(&snap, 202)));
    }

    // ---- invalid regex is non-constraining ----

    #[test]
    fn invalid_regex_matches_everything() {
        let snap = fixture();
        let q = compiled("fire(");
        assert!(q.has_invalid());
        for p in &snap.processes {
            assert!(matches(&q, p));
        }
    }

    #[test]
    fn invalid_regex_skipped_within_and_group() {
        let snap = fixture();
        // One valid + one invalid term in the same AND-group: the invalid term
        // is skipped, so only the valid `firefox` constrains.
        let q = compiled("firefox fire(");
        assert!(q.has_invalid());
        assert!(matches(&q, pid_of(&snap, 202)));
        assert!(!matches(&q, pid_of(&snap, 303)));
    }
}
