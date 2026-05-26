# Birb System Monitor

> [!IMPORTANT]
> **Disclaimer:** A significant portion of this codebase was generated with assistance from AI. While functional and reviewed, it may contain patterns that are not idiomatic or optimal.

A cross-platform system monitoring GUI built with [egui](https://github.com/emilk/egui) and [sysinfo](https://github.com/GuillaumeGomez/sysinfo-rs).

## Features

- **CPU** — Global and per-core usage with real-time stacked area chart
- **Memory & Swap** — Usage history with per-process overlay
- **Processes** — Searchable, sortable process table with multi-select
- **Network I/O** — Receive/transmit rate chart
- **Disk I/O** — Read/write rate chart
- **Temperatures** — Sensor readings with color-coded progress bars and history chart
- **Dashboard** — All graphs in a responsive auto-layout view
- **Dockable panels** — Freely arrange, resize, and tab panels with egui_dock
- **Persistent layout** — Panel arrangement and settings are saved between sessions

## Usage

```bash
cargo run --release
```

Add panels via the **Panels** menu. Drag and rearrange tabs to your liking.
