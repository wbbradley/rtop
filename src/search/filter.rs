use std::cmp::Ordering;

use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::{
    app::event::SortKey,
    process::{Process, Snapshot},
    search::parser::{Field, Query, Term},
};

pub fn filter(query: &Query, snap: &Snapshot, sort: SortKey) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..snap.processes.len()).collect();

    if !query.terms.is_empty() {
        let mut matcher = Matcher::new(Config::DEFAULT);
        let mut hay_buf: Vec<char> = Vec::new();
        let mut needle_buf: Vec<char> = Vec::new();

        indices.retain(|&i| {
            let p = &snap.processes[i];
            for term in &query.terms {
                if !term_matches(p, term, &mut matcher, &mut hay_buf, &mut needle_buf) {
                    return false;
                }
            }
            true
        });
    }

    sort_indices(&mut indices, snap, sort);
    indices
}

fn term_matches(
    p: &Process,
    term: &Term,
    matcher: &mut Matcher,
    hay_buf: &mut Vec<char>,
    needle_buf: &mut Vec<char>,
) -> bool {
    match term {
        Term::Prefixed { field, value } => match field {
            // pid: does NOT filter — it only sets auto_select_pid in the parsed query.
            Field::Pid => true,
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
            let haystack_str = format!("{} {} {}", p.name, cmdline_joined(p), p.user);
            hay_buf.clear();
            needle_buf.clear();
            let haystack = Utf32Str::new(&haystack_str, hay_buf);
            let needle = Utf32Str::new(s, needle_buf);
            matcher.fuzzy_match(haystack, needle).is_some()
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

fn sort_indices(indices: &mut [usize], snap: &Snapshot, sort: SortKey) {
    indices.sort_by(|&a, &b| compare(&snap.processes[a], &snap.processes[b], sort));
}

fn compare(a: &Process, b: &Process, sort: SortKey) -> Ordering {
    match sort {
        SortKey::Cpu => match (a.cpu_pct, b.cpu_pct) {
            (Some(x), Some(y)) => y.partial_cmp(&x).unwrap_or(Ordering::Equal),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        },
        SortKey::Rss => b.rss_bytes.cmp(&a.rss_bytes),
        SortKey::TimePlus => b.cpu_time_total.cmp(&a.cpu_time_total),
        SortKey::Age => b.age.cmp(&a.age),
    }
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

    fn pids(snap: &Snapshot, idxs: &[usize]) -> Vec<i32> {
        idxs.iter().map(|&i| snap.processes[i].id.pid).collect()
    }

    #[test]
    fn empty_query_returns_all() {
        let snap = fixture();
        let q = crate::search::parse("");
        let r = filter(&q, &snap, SortKey::Cpu);
        assert_eq!(r.len(), snap.processes.len());
    }

    #[test]
    fn user_filter() {
        let snap = fixture();
        let q = crate::search::parse("user:wbbradley");
        let r = filter(&q, &snap, SortKey::Cpu);
        let result_pids = pids(&snap, &r);
        assert!(result_pids.contains(&101));
        assert!(result_pids.contains(&202));
        assert!(result_pids.contains(&303));
        assert!(!result_pids.contains(&1));
        assert!(!result_pids.contains(&42));
    }

    #[test]
    fn name_filter() {
        let snap = fixture();
        let q = crate::search::parse("name:bash");
        let r = filter(&q, &snap, SortKey::Cpu);
        assert_eq!(pids(&snap, &r), vec![101]);
    }

    #[test]
    fn state_filter() {
        let snap = fixture();
        let q = crate::search::parse("state:R");
        let r = filter(&q, &snap, SortKey::Cpu);
        assert_eq!(pids(&snap, &r), vec![202]);
    }

    #[test]
    fn bare_fuzzy_matches_cmdline() {
        let snap = fixture();
        let q = crate::search::parse("firef");
        let r = filter(&q, &snap, SortKey::Cpu);
        assert_eq!(pids(&snap, &r), vec![202]);
    }

    #[test]
    fn two_term_and() {
        let snap = fixture();
        let q = crate::search::parse("user:wbbradley name:vim");
        let r = filter(&q, &snap, SortKey::Cpu);
        assert_eq!(pids(&snap, &r), vec![303]);
    }

    #[test]
    fn pid_does_not_filter() {
        let snap = fixture();
        let q = crate::search::parse("pid:42");
        let r = filter(&q, &snap, SortKey::Cpu);
        assert_eq!(r.len(), snap.processes.len());
        assert_eq!(q.auto_select_pid, Some(42));
    }

    #[test]
    fn ppid_filters_normally() {
        let snap = fixture();
        let q = crate::search::parse("ppid:1");
        let r = filter(&q, &snap, SortKey::Cpu);
        let result_pids: Vec<i32> = pids(&snap, &r).into_iter().collect();
        // pids whose ppid == 1: 42, 101, 303
        assert_eq!(result_pids.len(), 3);
        assert!(result_pids.contains(&42));
        assert!(result_pids.contains(&101));
        assert!(result_pids.contains(&303));
    }
}
