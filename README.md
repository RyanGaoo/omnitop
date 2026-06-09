# omnitop

A unified system monitor for the modern stack: **processes, containers, GPU, and per-process network — one pane of glass, one TUI.**

Existing monitors each show a slice: `htop`/`btop` show processes, `docker stats` shows containers, `nvidia-smi`/`asitop` show GPU, `nethogs` shows network. omnitop's goal is all of it, in one fast, beautiful terminal UI.

## Status

**v0.2 — interactive process monitoring** (working): live process table, search/filter, kill processes, CPU/memory gauges with history sparklines, per-core bars, sorting, pause.

## Install / Run

```sh
cargo run --release
```

## Keybindings

| Key                | Action                                               |
| ------------------ | ---------------------------------------------------- |
| `q` / `Esc`        | Quit                                                 |
| `↑`/`↓` or `j`/`k` | Navigate process list                                |
| `/`                | Filter by name or PID (Enter to apply, Esc to clear) |
| `x`                | Kill selected process with SIGTERM (confirmation)    |
| `X`                | Force-kill with SIGKILL (confirmation)               |
| `space`            | Pause/resume refresh                                 |
| `c`                | Sort by CPU                                          |
| `m`                | Sort by memory                                       |
| `p`                | Sort by PID                                          |
| `n`                | Sort by name                                         |

## Roadmap

- **v0.1 — Processes** ✅
  - Live process table (PID, name, CPU%, memory)
  - Global CPU + memory gauges
  - Sorting and keyboard navigation
- **v0.2 — Interactivity** ✅
  - Process search/filter (`/`)
  - Kill processes with confirmation (`x`)
  - Per-core CPU bars, CPU/memory history sparklines
  - Pause/resume (`space`)
- **v0.25 — Polish**
  - Process tree view
  - Config file (refresh rate, theme)
- **v0.3 — Containers**
  - Docker/Podman socket integration
  - Container CPU/mem vs. limits, restart counts
  - Map host processes to their containers
- **v0.4 — GPU**
  - NVIDIA via NVML, Apple Silicon via IOKit/powermetrics
  - Per-process GPU utilization and VRAM
- **v0.5 — Network**
  - Per-process network throughput (eBPF on Linux, nettop sources on macOS)
- **v1.0 — Release**
  - Homebrew tap, prebuilt binaries, AUR package
  - Benchmarks: omnitop's own overhead vs. htop/btop

## Architecture

```
src/
  main.rs   — event loop (input + 1s refresh tick)
  app.rs    — application state, sampling via sysinfo, sorting
  ui.rs     — ratatui rendering (header gauges, process table, footer)
```

The `sysinfo` crate is the initial sampling backend; the plan is to replace hot paths with direct `/proc` (Linux) and `libproc` (macOS) readers as profiling demands.

## License

MIT
