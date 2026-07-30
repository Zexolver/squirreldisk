use sysinfo::Disks;

/// A mounted disk/volume, decoupled from the UI layer.
pub struct RawDisk {
    pub mount_point: String,
    pub total_space: u64,
    pub available_space: u64,
    pub is_removable: bool,
}

/// Lists mounted disks, filtering out a handful of noisy mount points that
/// aren't useful to show to end users (mirrors the original app's behaviour).
pub fn list_disks() -> Vec<RawDisk> {
    let disks = Disks::new_with_refreshed_list();

    let mut out: Vec<RawDisk> = disks
        .list()
        .iter()
        .map(|disk| RawDisk {
            mount_point: disk.mount_point().display().to_string(),
            total_space: disk.total_space(),
            available_space: disk.available_space(),
            is_removable: disk.is_removable(),
        })
        .filter(|disk| !is_ignored_mount(&disk.mount_point))
        .filter(|disk| disk.total_space > 0)
        .collect();

    out.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));
    out
}

fn is_ignored_mount(mount_point: &str) -> bool {
    const IGNORED: &[&str] = &[
        "/System/Volumes/Data",
        "/var/snap/firefox/common/host-hunspell",
        "/boot/efi",
    ];
    IGNORED.contains(&mount_point)
}
