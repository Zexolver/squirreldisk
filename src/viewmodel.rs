use crate::scan::Tree;

/// One entry shown in either the treemap or the sidebar file list.
/// `node_id` is a real arena index, or `-1` for the synthetic "Smaller
/// items" bucket that aggregates children too small to be worth their own
/// cell (mirrors the original app's sunburst accumulator).
pub struct DisplayEntry {
    pub node_id: i32,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
}

const SMALL_ITEM_THRESHOLD: f64 = 0.01;

pub fn build_display_entries(tree: &Tree, focus: usize) -> Vec<DisplayEntry> {
    let node = &tree.nodes[focus];
    let total = node.size.max(1) as f64;

    let mut entries = Vec::new();
    let mut small_total = 0u64;
    let mut small_count = 0u32;

    for &child in &node.children {
        let child_node = &tree.nodes[child];
        if child_node.size == 0 {
            continue;
        }
        if (child_node.size as f64) / total > SMALL_ITEM_THRESHOLD {
            entries.push(DisplayEntry {
                node_id: child as i32,
                name: child_node.name.clone(),
                size: child_node.size,
                is_dir: child_node.is_dir,
            });
        } else {
            small_total += child_node.size;
            small_count += 1;
        }
    }

    if small_count > 0 {
        entries.push(DisplayEntry {
            node_id: -1,
            name: format!("Smaller items ({small_count})"),
            size: small_total,
            is_dir: false,
        });
    }

    entries.sort_by(|a, b| b.size.cmp(&a.size));
    entries
}
