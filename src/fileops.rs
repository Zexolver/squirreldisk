use std::io;
use std::path::Path;
use std::process::Command;

/// Reveals `path` in the OS's file manager, selecting it when the platform
/// supports that (Explorer, Finder). On Linux this falls back to opening the
/// containing folder, since there's no portable "select this file" API.
pub fn show_in_folder(path: &Path) {
    #[cfg(target_os = "windows")]
    {
        let arg = path.to_string_lossy().replace('/', "\\");
        if let Err(err) = Command::new("explorer").args(["/select,", &arg]).spawn() {
            eprintln!("failed to open explorer for {path:?}: {err}");
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Err(err) = Command::new("open").args(["-R", &path.to_string_lossy()]).spawn() {
            eprintln!("failed to reveal {path:?} in Finder: {err}");
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let target = match std::fs::metadata(path) {
            Ok(meta) if meta.is_dir() => path.to_path_buf(),
            _ => path.parent().map(Path::to_path_buf).unwrap_or_else(|| path.to_path_buf()),
        };
        if let Err(err) = Command::new("xdg-open").arg(&target).spawn() {
            eprintln!("failed to xdg-open {target:?}: {err}");
        }
    }
}

/// Recursively deletes a file or directory.
pub fn delete_path(path: &Path) -> io::Result<()> {
    let meta = std::fs::symlink_metadata(path)?;
    if meta.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}
