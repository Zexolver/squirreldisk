use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

/// A single file or directory in the scanned tree, stored in a flat arena so
/// that navigating to a parent is a cheap index lookup instead of requiring
/// back-references inside an owned recursive structure.
pub struct Node {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub parent: Option<usize>,
    /// Populated once the subtree has finished scanning, sorted by size (desc).
    pub children: Vec<usize>,
}

pub struct Tree {
    pub nodes: Vec<Node>,
    pub root: usize,
    pub root_path: PathBuf,
}

impl Tree {
    pub fn full_path(&self, idx: usize) -> PathBuf {
        let mut names = Vec::new();
        let mut cur = idx;
        while let Some(parent) = self.nodes[cur].parent {
            names.push(self.nodes[cur].name.as_str());
            cur = parent;
        }
        names.reverse();
        let mut path = self.root_path.clone();
        for name in names {
            path.push(name);
        }
        path
    }

    /// Removes `idx` from its parent's children and subtracts its size from
    /// every ancestor. The node itself is left as an orphaned, zero-size leaf
    /// in the arena (existing indices elsewhere must stay valid).
    pub fn remove(&mut self, idx: usize) {
        let size = self.nodes[idx].size;
        if let Some(parent) = self.nodes[idx].parent {
            self.nodes[parent].children.retain(|&c| c != idx);
            let mut cur = Some(parent);
            while let Some(p) = cur {
                self.nodes[p].size -= size;
                cur = self.nodes[p].parent;
            }
        }
        self.nodes[idx].size = 0;
        self.nodes[idx].children.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(name: &str, size: u64, parent: Option<usize>) -> Node {
        Node { name: name.to_string(), size, is_dir: parent.is_none() || size == 0, parent, children: Vec::new() }
    }

    // root(0) -> a(1, size 30) -> b(2, size 10), c(3, size 20)
    fn sample_tree() -> Tree {
        let mut nodes = vec![node("root", 30, None), node("a", 30, Some(0)), node("b", 10, Some(1)), node("c", 20, Some(1))];
        nodes[0].is_dir = true;
        nodes[1].is_dir = true;
        nodes[0].children = vec![1];
        nodes[1].children = vec![2, 3];
        Tree { nodes, root: 0, root_path: PathBuf::from("/tmp/root") }
    }

    #[test]
    fn full_path_walks_up_to_root() {
        let tree = sample_tree();
        assert_eq!(tree.full_path(0), PathBuf::from("/tmp/root"));
        assert_eq!(tree.full_path(1), PathBuf::from("/tmp/root/a"));
        assert_eq!(tree.full_path(3), PathBuf::from("/tmp/root/a/c"));
    }

    #[test]
    fn remove_subtracts_from_all_ancestors() {
        let mut tree = sample_tree();
        tree.remove(3); // remove "c" (size 20) from under "a"
        assert_eq!(tree.nodes[1].size, 10); // a: 30 - 20
        assert_eq!(tree.nodes[0].size, 10); // root: 30 - 20
        assert!(!tree.nodes[1].children.contains(&3));
        assert_eq!(tree.nodes[3].size, 0);
    }

    #[test]
    fn banned_root_entries_filters_expected_names() {
        for name in ["dev", "proc", "sys", "mnt"] {
            assert!(BANNED_ROOT_ENTRIES.contains(&name));
        }
        assert!(!BANNED_ROOT_ENTRIES.contains(&"home"));
    }
}

#[derive(Default)]
pub struct ScanProgress {
    pub items: AtomicU64,
    pub bytes: AtomicU64,
    pub errors: AtomicU64,
    pub done: AtomicBool,
}

pub struct ScanHandle {
    pub progress: Arc<ScanProgress>,
    pub cancelled: Arc<AtomicBool>,
    pub result: Arc<Mutex<Option<Tree>>>,
}

impl ScanHandle {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

/// Well-known virtual / system mount points that are not useful (and can be
/// dangerous, e.g. /proc) to walk into during a *quick* root scan. A "full
/// scan" (triggered by right-clicking a disk) skips this filter entirely.
const BANNED_ROOT_ENTRIES: &[&str] = &[
    "dev", "proc", "sys", "run", "mnt", "cdrom", "media", "Volumes", "System", "snap",
];

/// Starts scanning `path` on a background thread. `expected_total_bytes` is
/// used by the UI to render a determinate progress bar (pass `None` when
/// unknown, e.g. for an arbitrary user-picked folder). `is_root_scan` plus
/// `full` control whether virtual system directories are skipped.
pub fn start(
    path: PathBuf,
    is_root_scan: bool,
    full: bool,
) -> ScanHandle {
    let progress = Arc::new(ScanProgress::default());
    let cancelled = Arc::new(AtomicBool::new(false));
    let result = Arc::new(Mutex::new(None));

    let progress_bg = progress.clone();
    let cancelled_bg = cancelled.clone();
    let result_bg = result.clone();

    thread::spawn(move || {
        let mut nodes = Vec::new();
        let root_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());

        let skip_banned = is_root_scan && !full;
        scan_recursive(
            &mut nodes,
            None,
            root_name,
            &path,
            true,
            skip_banned,
            &progress_bg,
            &cancelled_bg,
        );

        let tree = Tree {
            nodes,
            root: 0,
            root_path: path,
        };
        *result_bg.lock().unwrap() = Some(tree);
        progress_bg.done.store(true, Ordering::Relaxed);
    });

    ScanHandle {
        progress,
        cancelled,
        result,
    }
}

#[allow(clippy::too_many_arguments)]
fn scan_recursive(
    arena: &mut Vec<Node>,
    parent: Option<usize>,
    name: String,
    path: &Path,
    is_top_level: bool,
    skip_banned: bool,
    progress: &ScanProgress,
    cancelled: &AtomicBool,
) -> usize {
    let idx = arena.len();
    arena.push(Node {
        name,
        size: 0,
        is_dir: false,
        parent,
        children: Vec::new(),
    });

    if cancelled.load(Ordering::Relaxed) {
        return idx;
    }

    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => {
            progress.errors.fetch_add(1, Ordering::Relaxed);
            return idx;
        }
    };

    if meta.is_symlink() {
        // Don't follow symlinks: avoids cycles and double-counting space.
        progress.items.fetch_add(1, Ordering::Relaxed);
        return idx;
    }

    if meta.is_dir() {
        arena[idx].is_dir = true;
        let read_dir = match fs::read_dir(path) {
            Ok(rd) => rd,
            Err(_) => {
                progress.errors.fetch_add(1, Ordering::Relaxed);
                return idx;
            }
        };

        let mut children = Vec::new();
        let mut total = 0u64;
        for entry in read_dir.flatten() {
            if cancelled.load(Ordering::Relaxed) {
                break;
            }
            let child_name = entry.file_name().to_string_lossy().to_string();
            if is_top_level && skip_banned && BANNED_ROOT_ENTRIES.contains(&child_name.as_str()) {
                continue;
            }
            let child_path = entry.path();
            let child_idx = scan_recursive(
                arena,
                Some(idx),
                child_name,
                &child_path,
                false,
                skip_banned,
                progress,
                cancelled,
            );
            total += arena[child_idx].size;
            children.push(child_idx);
        }
        children.sort_by(|&a, &b| arena[b].size.cmp(&arena[a].size));
        arena[idx].children = children;
        arena[idx].size = total;
    } else {
        arena[idx].size = meta.len();
        progress.bytes.fetch_add(arena[idx].size, Ordering::Relaxed);
    }

    progress.items.fetch_add(1, Ordering::Relaxed);
    idx
}
