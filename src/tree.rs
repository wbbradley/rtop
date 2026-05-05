use std::collections::HashMap;

use crate::process::{ProcessId, Snapshot};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GutterKind {
    Spine,
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
    /// the last child at its level (render `   `); false ⇒ render `│  `. For
    /// Spine nodes this is all-true (no vertical bars between spine entries).
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

pub fn build_visible(
    snap: &Snapshot,
    parent_to_children: &HashMap<i32, Vec<usize>>,
    pid_to_idx: &HashMap<i32, usize>,
    selected: ProcessId,
) -> Vec<TreeNode> {
    let Some(&selected_idx) = snap.by_id.get(&selected) else {
        return Vec::new();
    };

    // Walk the parent chain.
    let mut chain: Vec<usize> = vec![selected_idx];
    let mut cur_idx = selected_idx;
    let mut seen: std::collections::HashSet<usize> = std::collections::HashSet::new();
    seen.insert(cur_idx);
    loop {
        let p = &snap.processes[cur_idx];
        if p.ppid == 0 {
            break;
        }
        let Some(&parent_idx) = pid_to_idx.get(&p.ppid) else {
            break;
        };
        if parent_idx == cur_idx {
            break;
        }
        if !seen.insert(parent_idx) {
            break;
        }
        chain.push(parent_idx);
        cur_idx = parent_idx;
    }
    chain.reverse();

    let mut out: Vec<TreeNode> = Vec::with_capacity(chain.len() + 8);
    for (depth, &idx) in chain.iter().enumerate() {
        out.push(TreeNode {
            proc_idx: idx,
            depth,
            gutter_kind: GutterKind::Spine,
            is_last_child: true,
            ancestors_last: vec![true; depth],
        });
    }

    // DFS over selected's subtree.
    // Stack frames: (proc_idx, depth, is_last_child, ancestors_last_clone) — but
    // we'll iterate children manually rather than push/pop frames.
    let base_depth = chain.len();
    let mut ancestors_last: Vec<bool> = vec![true; base_depth];

    fn dfs(
        snap: &Snapshot,
        parent_to_children: &HashMap<i32, Vec<usize>>,
        node_idx: usize,
        depth: usize,
        ancestors_last: &mut Vec<bool>,
        out: &mut Vec<TreeNode>,
    ) {
        let pid = snap.processes[node_idx].id.pid;
        let Some(children) = parent_to_children.get(&pid) else {
            return;
        };
        let n = children.len();
        for (i, &child_idx) in children.iter().enumerate() {
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
                child_idx,
                depth + 1,
                ancestors_last,
                out,
            );
            ancestors_last.pop();
        }
    }

    dfs(
        snap,
        parent_to_children,
        selected_idx,
        base_depth,
        &mut ancestors_last,
        &mut out,
    );

    out
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
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

    fn id_for(snap: &Snapshot, pid: i32) -> ProcessId {
        snap.processes.iter().find(|p| p.id.pid == pid).unwrap().id
    }

    #[test]
    fn build_visible_root_selected() {
        let snap = fixture_simple();
        let p2c = build_parent_to_children(&snap);
        let pid_to_idx = build_pid_to_idx(&snap);
        let v = build_visible(&snap, &p2c, &pid_to_idx, id_for(&snap, 1));
        let pids: Vec<i32> = v.iter().map(|n| pid_at(&snap, n)).collect();
        assert_eq!(pids, vec![1, 2, 3, 4]);
        let kinds: Vec<GutterKind> = v.iter().map(|n| n.gutter_kind).collect();
        assert_eq!(
            kinds,
            vec![
                GutterKind::Spine,
                GutterKind::Branch,
                GutterKind::Leaf,
                GutterKind::Leaf,
            ]
        );
        let depths: Vec<usize> = v.iter().map(|n| n.depth).collect();
        assert_eq!(depths, vec![0, 1, 1, 2]);
    }

    #[test]
    fn build_visible_mid_selected() {
        let snap = fixture_simple();
        let p2c = build_parent_to_children(&snap);
        let pid_to_idx = build_pid_to_idx(&snap);
        let v = build_visible(&snap, &p2c, &pid_to_idx, id_for(&snap, 3));
        let pids: Vec<i32> = v.iter().map(|n| pid_at(&snap, n)).collect();
        assert_eq!(pids, vec![1, 3, 4]);
        let kinds: Vec<GutterKind> = v.iter().map(|n| n.gutter_kind).collect();
        assert_eq!(
            kinds,
            vec![GutterKind::Spine, GutterKind::Spine, GutterKind::Leaf]
        );
        let depths: Vec<usize> = v.iter().map(|n| n.depth).collect();
        assert_eq!(depths, vec![0, 1, 2]);
    }

    #[test]
    fn build_visible_leaf_selected() {
        let snap = fixture_simple();
        let p2c = build_parent_to_children(&snap);
        let pid_to_idx = build_pid_to_idx(&snap);
        let v = build_visible(&snap, &p2c, &pid_to_idx, id_for(&snap, 4));
        let pids: Vec<i32> = v.iter().map(|n| pid_at(&snap, n)).collect();
        assert_eq!(pids, vec![1, 3, 4]);
        let kinds: Vec<GutterKind> = v.iter().map(|n| n.gutter_kind).collect();
        assert_eq!(
            kinds,
            vec![GutterKind::Spine, GutterKind::Spine, GutterKind::Spine]
        );
        let depths: Vec<usize> = v.iter().map(|n| n.depth).collect();
        assert_eq!(depths, vec![0, 1, 2]);
    }

    #[test]
    fn build_visible_missing_selection() {
        let snap = fixture_simple();
        let p2c = build_parent_to_children(&snap);
        let pid_to_idx = build_pid_to_idx(&snap);
        let bogus = ProcessId {
            pid: 9999,
            start_time: 9999,
        };
        let v = build_visible(&snap, &p2c, &pid_to_idx, bogus);
        assert!(v.is_empty());
    }

    #[test]
    fn ancestors_last_flags_branch() {
        // root(1)
        //   ├─ A(10)         non-last
        //   │    ├─ C(20)    non-last child of A
        //   │    └─ D(21)    last child of A
        //   └─ B(11)         last
        // Select root → DFS lays out 1, A, C, D, B
        // D's ancestors_last == [true (col 0 ← root has no parent column? actually
        //   the column at depth 0 references root, and ancestors_last[0] is whether
        //   root was the last child at its level. Root is the only child of pseudo-
        //   parent 0, so it's last → true). col 1 ← A: A is NOT the last child of
        //   root → false. So D's ancestors_last == [true, false].
        let snap = snap_from(vec![
            mk_proc(1, 0),
            mk_proc(10, 1),
            mk_proc(11, 1),
            mk_proc(20, 10),
            mk_proc(21, 10),
        ]);
        let p2c = build_parent_to_children(&snap);
        let pid_to_idx = build_pid_to_idx(&snap);
        let v = build_visible(&snap, &p2c, &pid_to_idx, id_for(&snap, 1));
        // Order should be: 1, 10 (A), 20 (C), 21 (D), 11 (B)
        let pids: Vec<i32> = v.iter().map(|n| pid_at(&snap, n)).collect();
        assert_eq!(pids, vec![1, 10, 20, 21, 11]);

        let d_node = v.iter().find(|n| pid_at(&snap, n) == 21).unwrap();
        assert_eq!(d_node.depth, 2);
        assert_eq!(d_node.ancestors_last, vec![true, false]);
        assert_eq!(d_node.gutter_kind, GutterKind::Leaf);

        let c_node = v.iter().find(|n| pid_at(&snap, n) == 20).unwrap();
        assert_eq!(c_node.gutter_kind, GutterKind::Branch);
        assert_eq!(c_node.ancestors_last, vec![true, false]);

        let b_node = v.iter().find(|n| pid_at(&snap, n) == 11).unwrap();
        assert_eq!(b_node.gutter_kind, GutterKind::Leaf);
        assert_eq!(b_node.depth, 1);
        assert_eq!(b_node.ancestors_last, vec![true]);
    }
}
