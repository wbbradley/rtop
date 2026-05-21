use std::collections::{HashMap, HashSet, VecDeque};

use crate::process::Snapshot;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GutterKind {
    Branch,
    Leaf,
}

#[derive(Clone, Debug)]
pub struct TreeNode {
    pub proc_idx: usize,
    pub depth: usize,
    pub gutter_kind: GutterKind,
    #[allow(dead_code)]
    pub is_last_child: bool,
    /// One bool per ancestor column (length == depth). True ⇒ that ancestor was
    /// the last child at its level (render `   `); false ⇒ render `│  `. The
    /// flags refer to last-among-visible-kept-siblings, not raw children.
    pub ancestors_last: Vec<bool>,
}

pub fn build_pid_to_idx(snap: &Snapshot) -> HashMap<i32, usize> {
    snap.processes
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id.pid, i))
        .collect()
}

pub fn build_parent_to_children(snap: &Snapshot) -> HashMap<i32, Vec<usize>> {
    let mut m: HashMap<i32, Vec<usize>> = HashMap::with_capacity(snap.processes.len());
    for (i, p) in snap.processes.iter().enumerate() {
        m.entry(p.ppid).or_default().push(i);
    }
    for v in m.values_mut() {
        v.sort_by_key(|&i| snap.processes[i].id.pid);
    }
    m
}

/// Compute the visible forest.
///
/// - If `matched` is empty, the visible set is every process (minus kernel
///   threads when `hide_kernel_threads`).
/// - Otherwise, the visible set is the closure of `matched` extended with the
///   parent chain (root → match) and the complete subtree below each match.
///   When `hide_kernel_threads`, kernel-thread PIDs are excluded from `matched`
///   *and* skipped during the closure walk so they never appear in the result.
pub fn build_filtered(
    snap: &Snapshot,
    parent_to_children: &HashMap<i32, Vec<usize>>,
    pid_to_idx: &HashMap<i32, usize>,
    matched: &HashSet<i32>,
    hide_kernel_threads: bool,
) -> Vec<TreeNode> {
    let allowed = |idx: usize| !hide_kernel_threads || !snap.processes[idx].is_kernel_thread;

    let mut keep: HashSet<i32> = HashSet::with_capacity(snap.processes.len());

    if matched.is_empty() {
        for (i, p) in snap.processes.iter().enumerate() {
            if allowed(i) {
                keep.insert(p.id.pid);
            }
        }
    } else {
        let seeds: Vec<i32> = matched
            .iter()
            .copied()
            .filter(|pid| pid_to_idx.get(pid).is_some_and(|&idx| allowed(idx)))
            .collect();

        for &pid in &seeds {
            let Some(&start_idx) = pid_to_idx.get(&pid) else {
                continue;
            };
            let mut cur = start_idx;
            loop {
                if !allowed(cur) {
                    break;
                }
                let cur_pid = snap.processes[cur].id.pid;
                if !keep.insert(cur_pid) {
                    break;
                }
                let ppid = snap.processes[cur].ppid;
                if ppid == 0 {
                    break;
                }
                let Some(&parent_idx) = pid_to_idx.get(&ppid) else {
                    break;
                };
                if parent_idx == cur {
                    break;
                }
                cur = parent_idx;
            }
        }

        let mut queue: VecDeque<i32> = seeds.iter().copied().collect();
        while let Some(pid) = queue.pop_front() {
            let Some(children) = parent_to_children.get(&pid) else {
                continue;
            };
            for &child_idx in children {
                if !allowed(child_idx) {
                    continue;
                }
                let child_pid = snap.processes[child_idx].id.pid;
                if keep.insert(child_pid) {
                    queue.push_back(child_pid);
                }
            }
        }
    }

    if keep.is_empty() {
        return Vec::new();
    }

    let mut roots: Vec<usize> = Vec::new();
    for (i, p) in snap.processes.iter().enumerate() {
        if !keep.contains(&p.id.pid) {
            continue;
        }
        let parent_kept = p.ppid != 0 && pid_to_idx.contains_key(&p.ppid) && keep.contains(&p.ppid);
        if !parent_kept {
            roots.push(i);
        }
    }
    roots.sort_by_key(|&i| snap.processes[i].id.pid);

    let mut out: Vec<TreeNode> = Vec::new();
    let n_roots = roots.len();
    let mut ancestors_last: Vec<bool> = Vec::new();
    for (root_pos, &root_idx) in roots.iter().enumerate() {
        let is_last = root_pos + 1 == n_roots;
        out.push(TreeNode {
            proc_idx: root_idx,
            depth: 0,
            gutter_kind: GutterKind::Leaf,
            is_last_child: is_last,
            ancestors_last: Vec::new(),
        });
        dfs(
            snap,
            parent_to_children,
            &keep,
            root_idx,
            1,
            &mut ancestors_last,
            &mut out,
        );
    }

    out
}

fn dfs(
    snap: &Snapshot,
    parent_to_children: &HashMap<i32, Vec<usize>>,
    keep: &HashSet<i32>,
    node_idx: usize,
    depth: usize,
    ancestors_last: &mut Vec<bool>,
    out: &mut Vec<TreeNode>,
) {
    let pid = snap.processes[node_idx].id.pid;
    let Some(children) = parent_to_children.get(&pid) else {
        return;
    };
    let visible: Vec<usize> = children
        .iter()
        .copied()
        .filter(|&c| keep.contains(&snap.processes[c].id.pid))
        .collect();
    let n = visible.len();
    for (i, &child_idx) in visible.iter().enumerate() {
        let is_last = i + 1 == n;
        let kind = if is_last {
            GutterKind::Leaf
        } else {
            GutterKind::Branch
        };
        out.push(TreeNode {
            proc_idx: child_idx,
            depth,
            gutter_kind: kind,
            is_last_child: is_last,
            ancestors_last: ancestors_last.clone(),
        });
        ancestors_last.push(is_last);
        dfs(
            snap,
            parent_to_children,
            keep,
            child_idx,
            depth + 1,
            ancestors_last,
            out,
        );
        ancestors_last.pop();
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{HashMap, HashSet},
        time::{Duration, Instant},
    };

    use super::*;
    use crate::process::{Process, ProcessId, Snapshot, SystemStats};

    fn mk_proc(pid: i32, ppid: i32) -> Process {
        Process {
            id: ProcessId {
                pid,
                start_time: pid as u64,
            },
            ppid,
            uid: 0,
            user: "u".into(),
            name: format!("p{pid}"),
            cmdline: vec![format!("p{pid}")],
            state: 'S',
            rss_bytes: 0,
            cpu_pct: Some(0.0),
            cpu_time_total: Duration::ZERO,
            age: Duration::ZERO,
            is_kernel_thread: false,
        }
    }

    fn snap_from(procs: Vec<Process>) -> Snapshot {
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

    fn fixture_simple() -> Snapshot {
        // 1 → 2, 1 → 3, 3 → 4
        snap_from(vec![
            mk_proc(1, 0),
            mk_proc(2, 1),
            mk_proc(3, 1),
            mk_proc(4, 3),
        ])
    }

    fn pid_at(snap: &Snapshot, n: &TreeNode) -> i32 {
        snap.processes[n.proc_idx].id.pid
    }

    fn pids_in(snap: &Snapshot, v: &[TreeNode]) -> Vec<i32> {
        v.iter().map(|n| pid_at(snap, n)).collect()
    }

    fn build(snap: &Snapshot, matched: &[i32], hide_kernel_threads: bool) -> Vec<TreeNode> {
        let p2c = build_parent_to_children(snap);
        let pid_to_idx = build_pid_to_idx(snap);
        let set: HashSet<i32> = matched.iter().copied().collect();
        build_filtered(snap, &p2c, &pid_to_idx, &set, hide_kernel_threads)
    }

    #[test]
    fn build_filtered_empty_matched_shows_all() {
        let snap = fixture_simple();
        let v = build(&snap, &[], false);
        let pids = pids_in(&snap, &v);
        assert_eq!(pids, vec![1, 2, 3, 4]);
        let depths: Vec<usize> = v.iter().map(|n| n.depth).collect();
        assert_eq!(depths, vec![0, 1, 1, 2]);
    }

    #[test]
    fn build_filtered_single_leaf_match_shows_chain_only() {
        let snap = fixture_simple();
        let v = build(&snap, &[4], false);
        // Match on 4 → keep = {1, 3, 4}. 2 (sibling of 3, not on chain) hidden.
        let pids = pids_in(&snap, &v);
        assert_eq!(pids, vec![1, 3, 4]);
        // 3 is the only visible child of 1 → is_last_child=true, Leaf gutter.
        let three = v.iter().find(|n| pid_at(&snap, n) == 3).unwrap();
        assert!(three.is_last_child);
        assert_eq!(three.gutter_kind, GutterKind::Leaf);
        // 4 is the only visible child of 3.
        let four = v.iter().find(|n| pid_at(&snap, n) == 4).unwrap();
        assert!(four.is_last_child);
        assert_eq!(four.gutter_kind, GutterKind::Leaf);
        assert_eq!(four.ancestors_last, vec![true]);
    }

    #[test]
    fn build_filtered_internal_match_includes_subtree() {
        let snap = fixture_simple();
        let v = build(&snap, &[3], false);
        // Match on 3 → keep = ancestors(3) ∪ {3} ∪ descendants(3) = {1, 3, 4}.
        let pids = pids_in(&snap, &v);
        assert_eq!(pids, vec![1, 3, 4]);
    }

    #[test]
    fn build_filtered_disjoint_matches_yield_two_roots() {
        // Forest:
        //   1 (ppid=0)
        //     ├── 10
        //     └── 11
        //   20 (ppid=0)
        //     └── 30
        let snap = snap_from(vec![
            mk_proc(1, 0),
            mk_proc(10, 1),
            mk_proc(11, 1),
            mk_proc(20, 0),
            mk_proc(30, 20),
        ]);
        // Match 10 and 30: keep = {1, 10, 20, 30}. 11 hidden.
        let v = build(&snap, &[10, 30], false);
        let pids = pids_in(&snap, &v);
        assert_eq!(pids, vec![1, 10, 20, 30]);

        // Both roots have depth 0.
        let depths: Vec<usize> = v.iter().map(|n| n.depth).collect();
        assert_eq!(depths, vec![0, 1, 0, 1]);

        // 10 is the only visible child of 1 → Leaf, is_last=true.
        let ten = v.iter().find(|n| pid_at(&snap, n) == 10).unwrap();
        assert_eq!(ten.gutter_kind, GutterKind::Leaf);
        assert!(ten.is_last_child);
    }

    #[test]
    fn build_filtered_no_matches_yields_empty() {
        let snap = fixture_simple();
        let v = build(&snap, &[9999], false);
        assert!(v.is_empty());
    }

    #[test]
    fn build_filtered_ancestors_last_reflects_visible_siblings() {
        // 1
        // ├── 10
        // │    ├── 20
        // │    └── 21
        // └── 11
        let snap = snap_from(vec![
            mk_proc(1, 0),
            mk_proc(10, 1),
            mk_proc(11, 1),
            mk_proc(20, 10),
            mk_proc(21, 10),
        ]);
        // Match 20: keep = {1, 10, 20}. 11 and 21 hidden.
        let v = build(&snap, &[20], false);
        let pids = pids_in(&snap, &v);
        assert_eq!(pids, vec![1, 10, 20]);

        // With 21 hidden, 20 is now the only/last visible child of 10. Without
        // the visible-aware logic, 20 would still be marked non-last (since 21
        // exists in the raw children list).
        let twenty = v.iter().find(|n| pid_at(&snap, n) == 20).unwrap();
        assert!(twenty.is_last_child);
        assert_eq!(twenty.gutter_kind, GutterKind::Leaf);
        assert_eq!(twenty.ancestors_last, vec![true]);

        // With 11 hidden, 10 is also now last among visible children of 1.
        let ten = v.iter().find(|n| pid_at(&snap, n) == 10).unwrap();
        assert!(ten.is_last_child);
        assert_eq!(ten.gutter_kind, GutterKind::Leaf);
    }

    #[test]
    fn build_filtered_kthread_match_dropped_by_mask() {
        // 1 (regular), 2 (kthread root), 6 (kthread, child of 2).
        let mut procs = vec![mk_proc(1, 0), mk_proc(2, 0), mk_proc(6, 2)];
        procs[1].is_kernel_thread = true;
        procs[2].is_kernel_thread = true;
        let snap = snap_from(procs);

        // Match the kthread 6 with hide_kernel_threads=true → it's filtered out
        // of `matched` before closure; result is empty (no non-kthread is in
        // the chain).
        let v = build(&snap, &[6], true);
        assert!(v.is_empty(), "got {:?}", pids_in(&snap, &v));

        // Empty match with hide_kernel_threads=true → only the non-kthread 1.
        let v = build(&snap, &[], true);
        assert_eq!(pids_in(&snap, &v), vec![1]);

        // Without the mask, the kthread chain shows.
        let v = build(&snap, &[6], false);
        assert_eq!(pids_in(&snap, &v), vec![2, 6]);
    }

    #[test]
    fn build_filtered_multiple_roots_with_empty_matched() {
        // Two disjoint roots, no matches → both show.
        let snap = snap_from(vec![mk_proc(1, 0), mk_proc(2, 1), mk_proc(5, 0)]);
        let v = build(&snap, &[], false);
        // Order: root 1 + its subtree, then root 5. PID ascending.
        assert_eq!(pids_in(&snap, &v), vec![1, 2, 5]);
    }
}
