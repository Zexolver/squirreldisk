#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod disks;
mod fileops;
mod format;
mod scan;
mod treemap;
mod viewmodel;

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use slint::{Color, ComponentHandle, ModelRc, Timer, TimerMode, VecModel};

use disks::RawDisk;
use format::format_bytes;
use scan::Tree;
use viewmodel::DisplayEntry;

slint::include_modules!();

struct ScanRuntime {
    handle: scan::ScanHandle,
    expected_total: Option<u64>,
}

struct AppLogic {
    ui: slint::Weak<AppWindow>,
    tree: RefCell<Option<Tree>>,
    focus: Cell<usize>,
    delete_marks: RefCell<HashSet<i32>>,
    disks_cache: RefCell<Vec<RawDisk>>,
    canvas_size: Cell<(f32, f32)>,
    scan: RefCell<Option<ScanRuntime>>,
    scan_timer: RefCell<Option<Timer>>,
    delete_timer: RefCell<Option<Timer>>,
    disks_timer: RefCell<Option<Timer>>,
}

fn main() {
    let ui = AppWindow::new().expect("failed to create UI");
    ui.global::<AppState>()
        .set_app_version(env!("CARGO_PKG_VERSION").into());

    let logic = Rc::new(AppLogic {
        ui: ui.as_weak(),
        tree: RefCell::new(None),
        focus: Cell::new(0),
        delete_marks: RefCell::new(HashSet::new()),
        disks_cache: RefCell::new(Vec::new()),
        canvas_size: Cell::new((800.0, 600.0)),
        scan: RefCell::new(None),
        scan_timer: RefCell::new(None),
        delete_timer: RefCell::new(None),
        disks_timer: RefCell::new(None),
    });

    refresh_disks(&logic);

    {
        let timer = Timer::default();
        let logic_timer = logic.clone();
        timer.start(TimerMode::Repeated, Duration::from_secs(2), move || {
            if let Some(ui) = logic_timer.ui.upgrade() {
                if ui.global::<AppState>().get_view() == AppView::DiskList {
                    refresh_disks(&logic_timer);
                }
            }
        });
        *logic.disks_timer.borrow_mut() = Some(timer);
    }

    let state = ui.global::<AppState>();

    {
        let logic = logic.clone();
        state.on_select_folder(move || {
            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                start_scan(logic.clone(), folder, false, false, None, true);
            }
        });
    }

    {
        let logic = logic.clone();
        state.on_scan_disk(move |mount, full| {
            let mount = mount.to_string();
            let expected = logic
                .disks_cache
                .borrow()
                .iter()
                .find(|d| d.mount_point == mount)
                .map(|d| d.total_space.saturating_sub(d.available_space));
            start_scan(logic.clone(), PathBuf::from(mount), true, full, expected, false);
        });
    }

    {
        let logic = logic.clone();
        state.on_cancel_scan(move || {
            if let Some(scan) = logic.scan.borrow_mut().take() {
                scan.handle.cancel();
            }
            *logic.scan_timer.borrow_mut() = None;
        });
    }

    {
        let logic = logic.clone();
        state.on_back_to_list(move || {
            if let Some(ui) = logic.ui.upgrade() {
                ui.global::<AppState>().set_view(AppView::DiskList);
            }
        });
    }

    {
        let logic = logic.clone();
        state.on_focus_node(move |id| {
            if id < 0 {
                return;
            }
            let is_dir = logic
                .tree
                .borrow()
                .as_ref()
                .and_then(|t| t.nodes.get(id as usize))
                .map(|n| n.is_dir)
                .unwrap_or(false);
            if is_dir {
                logic.focus.set(id as usize);
                refresh_view(&logic);
            }
        });
    }

    {
        let logic = logic.clone();
        state.on_go_up(move || {
            let parent = logic
                .tree
                .borrow()
                .as_ref()
                .and_then(|t| t.nodes[logic.focus.get()].parent);
            if let Some(parent) = parent {
                logic.focus.set(parent);
                refresh_view(&logic);
            }
        });
    }

    {
        let logic = logic.clone();
        state.on_reveal_node(move |id| {
            if id < 0 {
                return;
            }
            let path = logic
                .tree
                .borrow()
                .as_ref()
                .map(|t| t.full_path(id as usize));
            if let Some(path) = path {
                fileops::show_in_folder(&path);
            }
        });
    }

    {
        let logic = logic.clone();
        state.on_reveal_current(move || {
            let focus = logic.focus.get();
            let path = logic.tree.borrow().as_ref().map(|t| t.full_path(focus));
            if let Some(path) = path {
                fileops::show_in_folder(&path);
            }
        });
    }

    {
        let logic = logic.clone();
        state.on_toggle_delete_mark(move |id| {
            if id < 0 {
                return;
            }
            {
                let mut marks = logic.delete_marks.borrow_mut();
                if !marks.remove(&id) {
                    marks.insert(id);
                }
            }
            refresh_view(&logic);
        });
    }

    {
        let logic = logic.clone();
        state.on_clear_delete_list(move || {
            logic.delete_marks.borrow_mut().clear();
            refresh_view(&logic);
        });
    }

    {
        let logic = logic.clone();
        state.on_confirm_delete(move || confirm_delete(logic.clone()));
    }

    {
        let logic = logic.clone();
        state.on_treemap_resized(move |w, h| {
            logic.canvas_size.set((w, h));
            refresh_view(&logic);
        });
    }

    ui.run().expect("failed to run UI");
}

fn refresh_disks(logic: &Rc<AppLogic>) {
    let Some(ui) = logic.ui.upgrade() else { return };
    let raw = disks::list_disks();

    let items: Vec<DiskInfo> = raw
        .iter()
        .map(|d| {
            let used = d.total_space.saturating_sub(d.available_space);
            let percent = if d.total_space > 0 {
                (used as f64 / d.total_space as f64 * 100.0) as f32
            } else {
                0.0
            };
            DiskInfo {
                mount_point: d.mount_point.clone().into(),
                total_text: format_bytes(d.total_space).into(),
                free_text: format_bytes(d.available_space).into(),
                used_percent: percent,
                is_removable: d.is_removable,
                bar_color: usage_color(percent),
            }
        })
        .collect();

    ui.global::<AppState>()
        .set_disks(ModelRc::new(VecModel::from(items)));
    *logic.disks_cache.borrow_mut() = raw;
}

fn usage_color(percent: f32) -> Color {
    if percent > 70.0 {
        Color::from_rgb_u8(220, 38, 38)
    } else if percent > 60.0 {
        Color::from_rgb_u8(202, 138, 4)
    } else {
        Color::from_rgb_u8(22, 163, 74)
    }
}

fn start_scan(
    logic: Rc<AppLogic>,
    path: PathBuf,
    is_root_scan: bool,
    full: bool,
    expected_total: Option<u64>,
    is_directory: bool,
) {
    let Some(ui) = logic.ui.upgrade() else { return };

    if let Some(prev) = logic.scan.borrow_mut().take() {
        prev.handle.cancel();
    }
    *logic.scan_timer.borrow_mut() = None;
    logic.delete_marks.borrow_mut().clear();
    *logic.tree.borrow_mut() = None;

    let state = ui.global::<AppState>();
    state.set_scan_path(path.display().to_string().into());
    state.set_scan_is_directory(is_directory);
    state.set_scan_progress_percent(0.0);
    state.set_scan_progress_text("Starting…".into());
    state.set_scan_had_error(false);
    state.set_view(AppView::Scanning);

    let handle = scan::start(path, is_root_scan, full);
    *logic.scan.borrow_mut() = Some(ScanRuntime { handle, expected_total });

    let timer = Timer::default();
    let logic_timer = logic.clone();
    timer.start(TimerMode::Repeated, Duration::from_millis(150), move || {
        tick_scan(&logic_timer);
    });
    *logic.scan_timer.borrow_mut() = Some(timer);
}

fn tick_scan(logic: &Rc<AppLogic>) {
    let Some(ui) = logic.ui.upgrade() else { return };

    let Some((items, bytes, errors, done, percent)) = ({
        let scan_ref = logic.scan.borrow();
        scan_ref.as_ref().map(|scan| {
            let items = scan.handle.progress.items.load(Ordering::Relaxed);
            let bytes = scan.handle.progress.bytes.load(Ordering::Relaxed);
            let errors = scan.handle.progress.errors.load(Ordering::Relaxed);
            let done = scan.handle.progress.done.load(Ordering::Relaxed);
            let percent = match scan.expected_total {
                Some(total) if total > 0 => {
                    ((bytes as f64 / total as f64) * 100.0).min(100.0) as f32
                }
                _ => 0.0,
            };
            (items, bytes, errors, done, percent)
        })
    }) else {
        return;
    };

    let state = ui.global::<AppState>();
    let mut text = format!("{items} items scanned · {}", format_bytes(bytes));
    if errors > 0 {
        text.push_str(&format!(" · {errors} skipped"));
    }
    state.set_scan_progress_text(text.into());
    state.set_scan_progress_percent(percent);

    if done {
        *logic.scan_timer.borrow_mut() = None;
        let tree = logic
            .scan
            .borrow_mut()
            .take()
            .and_then(|s| s.handle.result.lock().unwrap().take());

        match tree {
            Some(t) => {
                logic.focus.set(t.root);
                *logic.tree.borrow_mut() = Some(t);
                refresh_view(logic);
                state.set_view(AppView::Detail);
            }
            None => {
                state.set_scan_had_error(true);
                state.set_scan_error_text("The scan could not be completed.".into());
            }
        }
    }
}

fn confirm_delete(logic: Rc<AppLogic>) {
    let Some(ui) = logic.ui.upgrade() else { return };

    let marks: Vec<i32> = logic
        .delete_marks
        .borrow()
        .iter()
        .copied()
        .filter(|&id| id >= 0)
        .collect();
    if marks.is_empty() {
        return;
    }

    let paths: Vec<(i32, PathBuf)> = {
        let tree_ref = logic.tree.borrow();
        let Some(tree) = tree_ref.as_ref() else { return };
        marks.iter().map(|&id| (id, tree.full_path(id as usize))).collect()
    };

    let total = paths.len();
    let state = ui.global::<AppState>();
    state.set_deleting(true);
    state.set_deleting_status(format!("Deleting 0 of {total}").into());

    let progress = Arc::new(AtomicU64::new(0));
    let completed: Arc<Mutex<Vec<i32>>> = Arc::new(Mutex::new(Vec::new()));
    let done = Arc::new(AtomicBool::new(false));

    {
        let progress = progress.clone();
        let completed = completed.clone();
        let done = done.clone();
        thread::spawn(move || {
            for (id, path) in paths {
                if fileops::delete_path(&path).is_ok() {
                    completed.lock().unwrap().push(id);
                }
                progress.fetch_add(1, Ordering::Relaxed);
            }
            done.store(true, Ordering::Relaxed);
        });
    }

    let timer = Timer::default();
    let logic_timer = logic.clone();
    timer.start(TimerMode::Repeated, Duration::from_millis(120), move || {
        let current = progress.load(Ordering::Relaxed);
        if let Some(ui) = logic_timer.ui.upgrade() {
            ui.global::<AppState>()
                .set_deleting_status(format!("Deleting {current} of {total}").into());
        }

        if done.load(Ordering::Relaxed) {
            let ids = std::mem::take(&mut *completed.lock().unwrap());
            {
                let mut tree_mut = logic_timer.tree.borrow_mut();
                if let Some(tree) = tree_mut.as_mut() {
                    for id in &ids {
                        tree.remove(*id as usize);
                    }
                }
            }
            {
                let mut marks = logic_timer.delete_marks.borrow_mut();
                for id in &ids {
                    marks.remove(id);
                }
            }
            if let Some(ui) = logic_timer.ui.upgrade() {
                ui.global::<AppState>().set_deleting(false);
            }
            refresh_view(&logic_timer);
            *logic_timer.delete_timer.borrow_mut() = None;
        }
    });
    *logic.delete_timer.borrow_mut() = Some(timer);
}

const PALETTE: [(u8, u8, u8); 10] = [
    (99, 102, 241),
    (16, 185, 129),
    (245, 158, 11),
    (239, 68, 68),
    (59, 130, 246),
    (168, 85, 247),
    (236, 72, 153),
    (20, 184, 166),
    (132, 204, 22),
    (234, 179, 8),
];

fn color_for(node_id: i32, index: usize, is_dir: bool) -> Color {
    if node_id < 0 {
        return Color::from_rgb_u8(75, 85, 99);
    }
    let (r, g, b) = PALETTE[index % PALETTE.len()];
    if is_dir {
        Color::from_rgb_u8(r, g, b)
    } else {
        Color::from_rgb_u8((r as u16 * 7 / 10) as u8, (g as u16 * 7 / 10) as u8, (b as u16 * 7 / 10) as u8)
    }
}

fn refresh_view(logic: &Rc<AppLogic>) {
    let Some(ui) = logic.ui.upgrade() else { return };
    let tree_ref = logic.tree.borrow();
    let Some(tree) = tree_ref.as_ref() else { return };
    let focus = logic.focus.get();
    let node = &tree.nodes[focus];

    let state = ui.global::<AppState>();
    let full_path = tree.full_path(focus);
    state.set_breadcrumb(full_path.display().to_string().into());
    state.set_focused_size_text(format_bytes(node.size).into());
    state.set_can_go_up(node.parent.is_some());

    let entries = viewmodel::build_display_entries(tree, focus);
    let total = node.size.max(1) as f64;
    let marks = logic.delete_marks.borrow();

    let rows: Vec<FileRow> = entries
        .iter()
        .map(|e| FileRow {
            node_id: e.node_id,
            name: e.name.clone().into(),
            size_text: format_bytes(e.size).into(),
            is_dir: e.is_dir,
            percent: ((e.size as f64 / total) * 100.0) as f32,
            marked: marks.contains(&e.node_id),
        })
        .collect();
    state.set_file_rows(ModelRc::new(VecModel::from(rows)));

    let (delete_count, delete_size) = {
        let mut count = 0i32;
        let mut size = 0u64;
        for &id in marks.iter() {
            if id >= 0 {
                if let Some(n) = tree.nodes.get(id as usize) {
                    count += 1;
                    size += n.size;
                }
            }
        }
        (count, size)
    };
    drop(marks);
    state.set_delete_count(delete_count);
    state.set_delete_size_text(format_bytes(delete_size).into());

    let (w, h) = logic.canvas_size.get();
    let items: Vec<(i32, u64)> = entries.iter().map(|e| (e.node_id, e.size)).collect();
    let rects = treemap::layout(&items, treemap::Rect { x: 0.0, y: 0.0, w, h });

    let entry_by_id: std::collections::HashMap<i32, (usize, &DisplayEntry)> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| (e.node_id, (i, e)))
        .collect();

    let cells: Vec<TreemapRect> = rects
        .into_iter()
        .filter(|(_, r)| r.w > 1.0 && r.h > 1.0)
        .filter_map(|(id, r)| {
            let (index, entry) = entry_by_id.get(&id)?;
            Some(TreemapRect {
                node_id: id,
                x: r.x,
                y: r.y,
                width: r.w,
                height: r.h,
                color: color_for(id, *index, entry.is_dir),
                label: entry.name.clone().into(),
                size_text: format_bytes(entry.size).into(),
                is_dir: entry.is_dir,
            })
        })
        .collect();
    state.set_treemap_rects(ModelRc::new(VecModel::from(cells)));
}
