# Shazam Tray

macOS menu bar app for [Shazam](https://shazam.dev) — AI Agent Orchestrator.

Shazam Tray sits in your menu bar and gives you a persistent overview of all your Shazam projects. It manages the backend daemon, shows real-time project status, sends macOS notifications, and lets you open the TUI for any project with one click.

## What it does

- **Menu bar icon** with dynamic colors: gray (offline), yellow (idle), green (running), orange (approval pending), red (error)
- **Daemon management** — Start, stop, and restart the Shazam backend directly from the menu bar
- **Multi-project status** — See all active projects with agent counts, task stats, and cost
- **macOS notifications** — Get notified when tasks need approval or when failures occur
- **Sound alerts** — Audio feedback on important events
- **One-click TUI** — Open a terminal with Shazam running for any project
- **Auto-start on login** — Toggle launchd integration from the menu

## Architecture

```
┌──────────────────────────────────┐
│  shazam-tray (menu bar)          │
│  └── polls shazam-core daemon    │
│      via REST API (:4040)        │
└──────────────┬───────────────────┘
               │ HTTP polling
               ▼
┌──────────────────────────────────┐
│  shazam-core (Elixir daemon)     │
│  └── API :4040 (REST + WS)      │
│      manages companies, agents,  │
│      tasks, plugins              │
└──────────────────────────────────┘
```

The tray app polls `GET /api/health` every 3 seconds to get the list of active projects, then `GET /api/tasks` for task counts. It uses `shazam daemon start/stop` CLI commands to manage the backend lifecycle.

## Prerequisites

- macOS
- [Shazam CLI](https://github.com/ShazamAI/shazam-cli) installed (`shazam-cli` in PATH)
- [Rust](https://rustup.rs/) (for building from source)

## Install

### Via setup.sh (recommended)

The Shazam installer includes shazam-tray automatically:

```bash
curl -fsSL https://raw.githubusercontent.com/raphaelbarbosaqwerty/shazam-cli/main/setup.sh | bash
```

### From source

```bash
git clone https://github.com/ShazamAI/shazam-tray.git
cd shazam-tray
cargo build --release
cp target/release/shazam-tray ~/bin/
```

## Usage

```bash
# Run directly
shazam-tray

# Or if installed to ~/bin
shazam-tray
```

The lightning bolt icon appears in your menu bar. Click it to:

1. **Start Backend** — launches `shazam daemon start`
2. **View projects** — shows active projects with stats
3. **Click a project** — opens Terminal with `cd project && shazam`
4. **Start on Login** — toggles auto-start via launchd

## Menu structure

```
⚡ 2 project(s) | 3 agents | 1 tasks | $0.42
──────────────────────────────────────────────
▶  Start Backend
■  Stop Backend
↻  Restart Backend
──────────────────────────────────────────────
● ProjectA  — 2t/5  P:1 R:1 D:12 $0.30
● ProjectB  — 1t/3  P:0 R:0 D:8  $0.12
──────────────────────────────────────────────
  ↗ Open ProjectA in Terminal
  ↗ Open ProjectB in Terminal
Open New Terminal
✓ Start on Login
──────────────────────────────────────────────
Quit Shazam Tray
```

## Project structure

```
shazam-tray/
├── Cargo.toml           # Dependencies
├── LICENSE              # MIT
├── README.md
├── icon_*.png           # App icons (various sizes)
└── src/
    ├── main.rs          # Entry point — spawns threads, runs tray
    ├── tray.rs          # Menu bar UI, icon management, click handlers
    ├── daemon.rs        # Start/stop daemon via shazam CLI
    ├── ws_client.rs     # HTTP polling for project status
    └── protocol.rs      # JSON message types (shared with TUI)
```

## License

MIT — see [LICENSE](LICENSE)
