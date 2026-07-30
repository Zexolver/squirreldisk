# SquirrelDisk

<br>

<p align="center">
    <a href="https://github.com/adileo/squirreldisk"><img src="https://img.shields.io/github/v/release/adileo/squirreldisk?color=%23ff00a0&include_prereleases&label=version&sort=semver&style=flat-square"></a>
     &nbsp;
      <a href="https://github.com/adileo/squirreldisk"><img src="https://shields.io/badge/-ALPHA-orange?color=%23ff00a0&include_prereleases&label=status&sort=semver&style=flat-square"></a>
    &nbsp;
    <a href="https://github.com/adileo/squirreldisk"><img src="https://img.shields.io/badge/built_with-Rust-dca282.svg?style=flat-square"></a>
     &nbsp;
     <a href="https://discord.gg/Xp8QtMM65w"><img src="https://img.shields.io/badge/Discord-%235865F2.svg?style=flat-square&logo=discord&logoColor=white"></a>

</p>

<div align="center">

[![Windows Support](https://img.shields.io/badge/Windows-0078D6?style=for-the-badge&logo=windows&logoColor=white)](https://github.com/adileo/squirreldisk/releases) [![Ubuntu Support](https://img.shields.io/badge/Ubuntu-E95420?style=for-the-badge&logo=ubuntu&logoColor=white)](https://github.com/adileo/squirreldisk/releases) [![Windows Support](https://img.shields.io/badge/MACOS-adb8c5?style=for-the-badge&logo=macos&logoColor=white)](https://github.com/adileo/squirreldisk/releases)

</div>

## What's taking your hard disk space?

The easiest open source app you will ever use to detect huge files. Built entirely in Rust, with a native [Slint](https://slint.dev) UI — no webview, no JavaScript, no bundled browser runtime.

Squirreldisk is an open source alternative to software like: WinDirStat, WizTree, TreeSize and DaisyDisk.

Some features:

- Fast recursive directory scanning on a background thread, with a live progress bar
- Scan an entire disk, or pick any folder
- A treemap view to quickly visualize where your disk space is going
- Multi-select files/folders and delete them straight from the app
- Right-click a file/folder to reveal it in your OS file manager
- Cross-platform: macOS, Windows, Linux — one native binary, no runtime dependencies

## Architecture

This is a full rewrite of the original Tauri + React app. The backend is still Rust, but the entire UI layer — previously a React/TypeScript webview rendered by Tauri — has been replaced with [Slint](https://slint.dev), a native, GPU-accelerated Rust UI toolkit. The result is a single self-contained binary per platform with no embedded browser and no Node/npm build step.

```
squirreldisk/
├── Cargo.toml         # single binary crate
├── build.rs           # compiles the .slint UI at build time
├── ui/                 # Slint UI (.slint files)
│   ├── app.slint        # window shell + page router
│   ├── globals.slint     # shared state/structs exposed to Rust
│   ├── disk-list.slint   # "pick a disk / folder" screen
│   ├── scanning-page.slint
│   ├── detail-page.slint # treemap + file browser + delete flow
│   └── icons.slint       # small vector glyphs (no image assets needed)
├── icons/              # app icon (window icon + packaging)
└── src/
    ├── main.rs          # wires Slint callbacks/models to the Rust backend
    ├── disks.rs          # enumerates mounted disks (sysinfo)
    ├── scan.rs           # background recursive scanner (arena-based tree)
    ├── treemap.rs         # squarified treemap layout algorithm
    ├── viewmodel.rs        # builds the entries shown for a focused folder
    ├── fileops.rs          # reveal-in-file-manager / delete
    └── format.rs           # byte formatting helpers
```

### Notable design differences from the original app

- **Treemap instead of a zoomable sunburst.** The original used a D3.js arc/sunburst chart, which relies on animated SVG path interpolation that doesn't map cleanly onto Slint's declarative, retained-mode UI model. A squarified treemap (implemented from scratch in `src/treemap.rs`, no dependency) gives the same "see what's big at a glance" experience — it's also what WinDirStat itself uses — while being a natural fit for Slint's rectangle/`Repeater` model.
- **No bundled `pdu` sidecar binary.** Scanning is now done in-process, on a background thread, using a simple arena-allocated tree (`src/scan.rs`) instead of shelling out to an external `pdu` process and parsing its stdout/stderr like the Tauri version did.
- **Checkbox multi-select instead of drag-and-drop.** Dragging files into a "delete bin" was a `react-beautiful-dnd` feature specific to the web UI; Slint doesn't have an equivalent widget, so deletion now works by ticking a small checkbox next to each item, which is arguably more discoverable and works the same regardless of pointer precision.
- **Native window chrome instead of a custom vibrancy title bar.** The Tauri version drew its own title bar and used platform-specific window-vibrancy hacks. Those are Tauri/webview-specific integrations; SquirrelDisk now uses the OS's normal window frame, with an in-content breadcrumb bar for navigation.
- **No auto-updater.** The old updater was wired into Tauri's signed-update infrastructure. It has been dropped rather than half-ported; releases are plain per-OS binaries attached to GitHub Releases (see `.github/workflows/main.yml`).

## Building from source

Requires a recent stable Rust toolchain (`rustup` recommended).

```bash
cargo run            # debug build + run
cargo build --release
```

On Linux you'll need the usual Slint/winit windowing dependencies, e.g. on Debian/Ubuntu:

```bash
sudo apt-get install libfontconfig1-dev libxcb-xfixes0-dev libxcb-shape0-dev \
  libxkbcommon-dev libwayland-dev libgl1-mesa-dev libegl1-mesa-dev
```

## Installation

Please note that the current version is not 100% stable yet, and you may encounter bugs.

Download the binary for your platform from the [release page](https://github.com/adileo/squirreldisk/releases). Builds are not code-signed, so your OS may warn you before the first launch:

- **Windows**: click "More info" → "Run anyway". ([Why?](https://news.ycombinator.com/item?id=19330062))
- **macOS**: `Right click > Open` once (it will refuse to run and warn about an unsigned binary), then `Right click > Open` again to confirm — this is only needed the first time.
- **Linux**: mark the downloaded binary executable (`chmod +x`) and run it.

## Disclaimer

This app started as a project from a few years ago built in Electron in 2 days, then ported to Tauri, and is now a from-scratch rewrite on Rust + Slint. Yay.

## Bug Reporting

If you find any bugs, please report it by submitting an issue on our [issue page](https://github.com/adileo/squirreldisk/issues) with a detailed explanation. Giving some screenshots would also be very helpful.

## Feature Request

You can also submit a feature request on our [issue page](https://github.com/adileo/squirreldisk/issues) or [discussions](https://github.com/adileo/squirreldisk/discussions) and we will try to implement it as soon as possible.

## Contributions

- [Join our Discord Server](https://discord.gg/Xp8QtMM65w)

## Credits

- [Slint](https://github.com/slint-ui/slint)
- [sysinfo](https://github.com/GuillaumeGomez/sysinfo)
