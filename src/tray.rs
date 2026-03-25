use crate::AppState;
use crate::daemon;
use std::sync::{Arc, Mutex};

use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIconBuilder,
};

const MAX_PROJECTS: usize = 5;

// ── Icon builders ──────────────────────────────────────

fn build_icon_colored(r: u8, g: u8, b: u8) -> tray_icon::Icon {
    let png_bytes = include_bytes!("../icon_22x22.png");
    let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    let mut reader = decoder.read_info().expect("Failed to read PNG");
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("Failed to decode PNG");
    buf.truncate(info.buffer_size());

    for i in (0..buf.len()).step_by(4) {
        if buf[i + 3] > 50 {
            let brightness = (buf[i] as u16 + buf[i+1] as u16 + buf[i+2] as u16) / 3;
            let f = brightness as f32 / 255.0;
            buf[i]     = (r as f32 * f).min(255.0) as u8;
            buf[i + 1] = (g as f32 * f).min(255.0) as u8;
            buf[i + 2] = (b as f32 * f).min(255.0) as u8;
        }
    }

    tray_icon::Icon::from_rgba(buf, info.width, info.height).expect("Failed to create icon")
}

fn icon_default() -> tray_icon::Icon { build_icon_colored(255, 200, 0) }
fn icon_green() -> tray_icon::Icon   { build_icon_colored(0, 220, 80) }
fn icon_red() -> tray_icon::Icon     { build_icon_colored(255, 60, 60) }
fn icon_orange() -> tray_icon::Icon  { build_icon_colored(255, 160, 0) }
fn icon_gray() -> tray_icon::Icon    { build_icon_colored(120, 120, 120) }

#[derive(PartialEq, Clone)]
enum IconState { Offline, Idle, Running, Error, ApprovalPending }

// ── Main tray loop ─────────────────────────────────────

pub fn run_tray(state: Arc<Mutex<AppState>>) {
    let event_loop = EventLoopBuilder::new().build();
    let menu = Menu::new();

    // Status (non-clickable)
    let status_item = MenuItem::new("⚡ Shazam — Starting...", false, None);

    // Daemon controls
    let start_item = MenuItem::new("▶  Start Backend", true, None);
    let stop_item = MenuItem::new("■  Stop Backend", false, None);
    let restart_item = MenuItem::new("↻  Restart Backend", false, None);

    // Project slots (pre-allocated, hidden when empty)
    let project_items: Vec<MenuItem> = (0..MAX_PROJECTS)
        .map(|_| {
            let m = MenuItem::new("", true, None);
            m.set_enabled(false);
            m
        })
        .collect();

    // Open TUI per project (pre-allocated)
    let open_items: Vec<MenuItem> = (0..MAX_PROJECTS)
        .map(|_| {
            let m = MenuItem::new("", true, None);
            m.set_enabled(false);
            m
        })
        .collect();

    let open_tui_item = MenuItem::new("Open New Terminal", true, None);
    let notifications_item = MenuItem::new("✓ Notifications", true, None);
    let sounds_item = MenuItem::new("✓ Sounds", true, None);
    let autostart_item = MenuItem::new("Start on Login", true, None);
    let quit_item = MenuItem::new("Quit Shazam Tray", true, None);

    // Assemble menu
    let _ = menu.append(&status_item);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&start_item);
    let _ = menu.append(&stop_item);
    let _ = menu.append(&restart_item);
    let _ = menu.append(&PredefinedMenuItem::separator());
    for item in &project_items { let _ = menu.append(item); }
    let _ = menu.append(&PredefinedMenuItem::separator());
    for item in &open_items { let _ = menu.append(item); }
    let _ = menu.append(&open_tui_item);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&notifications_item);
    let _ = menu.append(&sounds_item);
    let _ = menu.append(&autostart_item);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&quit_item);

    let _tray = TrayIconBuilder::new()
        .with_icon(icon_gray())
        .with_menu(Box::new(menu))
        .with_tooltip("Shazam — AI Agent Orchestrator")
        .build()
        .expect("Failed to create tray icon");

    // IDs
    let start_id = start_item.id().clone();
    let stop_id = stop_item.id().clone();
    let restart_id = restart_item.id().clone();
    let open_tui_id = open_tui_item.id().clone();
    let notifications_id = notifications_item.id().clone();
    let sounds_id = sounds_item.id().clone();
    let autostart_id = autostart_item.id().clone();
    let quit_id = quit_item.id().clone();
    let proj_ids: Vec<_> = project_items.iter().map(|i| i.id().clone()).collect();
    let open_ids: Vec<_> = open_items.iter().map(|i| i.id().clone()).collect();

    let mut last_update = std::time::Instant::now();
    let mut current_icon = IconState::Offline;
    let mut prev_approvals: u32 = 0;
    let mut prev_failures: u32 = 0;

    // Autostart check
    let installed = is_autostart_installed();
    autostart_item.set_text(if installed { "✓ Start on Login" } else { "  Start on Login" });

    event_loop.run(move |_event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(
            std::time::Instant::now() + std::time::Duration::from_secs(1),
        );

        // ── Handle clicks ────────────────────────────────

        if let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == quit_id {
                *control_flow = ControlFlow::Exit;
                return;
            }
            if event.id == start_id {
                status_item.set_text("⚡ Shazam — Starting backend...");
                start_item.set_enabled(false);
                let sc = state.clone();
                std::thread::spawn(move || { daemon::ensure_daemon_running(sc); });
            }
            if event.id == stop_id {
                daemon::stop_daemon();
                if let Ok(mut s) = state.lock() { s.daemon_running = false; s.projects.clear(); }
            }
            if event.id == restart_id {
                status_item.set_text("⚡ Shazam — Restarting...");
                daemon::stop_daemon();
                let sc = state.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    daemon::ensure_daemon_running(sc);
                });
            }
            if event.id == open_tui_id {
                let _ = std::process::Command::new("open").args(["-a", "Terminal"]).spawn();
            }
            if event.id == notifications_id {
                if let Ok(mut s) = state.lock() {
                    s.notifications_enabled = !s.notifications_enabled;
                    notifications_item.set_text(if s.notifications_enabled { "✓ Notifications" } else { "  Notifications" });
                }
            }
            if event.id == sounds_id {
                if let Ok(mut s) = state.lock() {
                    s.sounds_enabled = !s.sounds_enabled;
                    sounds_item.set_text(if s.sounds_enabled { "✓ Sounds" } else { "  Sounds" });
                }
            }
            if event.id == autostart_id {
                toggle_autostart(&autostart_item);
            }
            // Project clicks → open TUI for that workspace
            for (i, pid) in proj_ids.iter().enumerate() {
                if event.id == *pid {
                    if let Ok(s) = state.lock() {
                        if let Some(p) = s.projects.get(i) {
                            open_tui_for_project(&p.workspace);
                        }
                    }
                }
            }
            // Open TUI item clicks per project
            for (i, oid) in open_ids.iter().enumerate() {
                if event.id == *oid {
                    if let Ok(s) = state.lock() {
                        if let Some(p) = s.projects.get(i) {
                            open_tui_for_project(&p.workspace);
                        }
                    }
                }
            }
        }

        // ── Periodic update ──────────────────────────────

        if last_update.elapsed() < std::time::Duration::from_secs(2) { return; }
        last_update = std::time::Instant::now();

        let Ok(s) = state.lock() else { return; };
        let running = s.daemon_running;

        // Buttons
        start_item.set_enabled(!running);
        stop_item.set_enabled(running);
        restart_item.set_enabled(running);

        // Icon state
        let new_icon = if !running {
            IconState::Offline
        } else if s.projects.is_empty() {
            IconState::Idle
        } else {
            let approvals = s.projects.iter().any(|p| p.tasks_pending > 0);
            let active = s.projects.iter().any(|p| p.tasks_running > 0 || p.agents_active > 0);
            if approvals { IconState::ApprovalPending }
            else if active { IconState::Running }
            else { IconState::Idle }
        };
        if new_icon != current_icon {
            let icon = match &new_icon {
                IconState::Offline => icon_gray(),
                IconState::Idle => icon_default(),
                IconState::Running => icon_green(),
                IconState::Error => icon_red(),
                IconState::ApprovalPending => icon_orange(),
            };
            let _ = _tray.set_icon(Some(icon));
            current_icon = new_icon;
        }

        // Status text
        let status_text = if !running {
            "⚡ Shazam — Backend offline".to_string()
        } else if s.projects.is_empty() {
            "⚡ Shazam — Running (no projects)".to_string()
        } else {
            let agents: u32 = s.projects.iter().map(|p| p.agents_active).sum();
            let tasks: u32 = s.projects.iter().map(|p| p.tasks_running).sum();
            let cost: f64 = s.projects.iter().map(|p| p.total_cost).sum();
            format!("⚡ {} project(s) | {} agents | {} tasks | ${:.2}", s.projects.len(), agents, tasks, cost)
        };
        status_item.set_text(&status_text);

        // Project items — show active, hide rest
        for (i, item) in project_items.iter().enumerate() {
            if let Some(p) = s.projects.get(i) {
                item.set_text(&format!(
                    "● {}  — {}t/{}  P:{} R:{} D:{} ${:.2}",
                    p.name, p.agents_active, p.agents_total,
                    p.tasks_pending, p.tasks_running, p.tasks_done, p.total_cost,
                ));
                item.set_enabled(true);
            } else if i == 0 && s.projects.is_empty() {
                item.set_text("  No active projects");
                item.set_enabled(false);
            } else {
                item.set_text("");
                item.set_enabled(false);
            }
        }

        // Open TUI items per project
        for (i, item) in open_items.iter().enumerate() {
            if let Some(p) = s.projects.get(i) {
                item.set_text(&format!("  ↗ Open {} in Terminal", p.name));
                item.set_enabled(true);
            } else {
                item.set_text("");
                item.set_enabled(false);
            }
        }

        // ── Notifications ────────────────────────────────

        let total_approvals: u32 = s.projects.iter().map(|p| p.tasks_pending).sum();
        if total_approvals > prev_approvals && prev_approvals > 0 {
            if s.notifications_enabled {
                send_notification("Shazam — Approval Needed",
                    &format!("{} task(s) awaiting approval", total_approvals));
            }
            if s.sounds_enabled {
                play_sound("Ping");
            }
        }
        prev_approvals = total_approvals;

        let total_failures: u32 = s.projects.iter().map(|p| p.tasks_done).sum(); // rough proxy
        // TODO: track actual failure count from API for precise notifications

        // Tooltip
        let tooltip = if s.projects.is_empty() {
            "Shazam — AI Agent Orchestrator".to_string()
        } else {
            s.projects.iter()
                .map(|p| format!("{}: {}a {}t ${:.2}", p.name, p.agents_active, p.tasks_running, p.total_cost))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let _ = _tray.set_tooltip(Some(&tooltip));
    });
}

// ── Helpers ────────────────────────────────────────────

fn open_tui_for_project(workspace: &str) {
    if workspace.is_empty() {
        let _ = std::process::Command::new("open").args(["-a", "Terminal"]).spawn();
        return;
    }
    let script = format!(
        "tell application \"Terminal\" to do script \"cd {} && shazam\"",
        workspace.replace("\"", "\\\"")
    );
    let _ = std::process::Command::new("osascript").args(["-e", &script]).spawn();
}

fn send_notification(title: &str, body: &str) {
    let _ = mac_notification_sys::Notification::default()
        .title(title)
        .message(body)
        .send();
}

fn play_sound(name: &str) {
    // Play macOS system sound (e.g. "Ping", "Glass", "Basso", "Hero")
    let _ = std::process::Command::new("afplay")
        .args([&format!("/System/Library/Sounds/{}.aiff", name)])
        .spawn();
}

fn is_autostart_installed() -> bool {
    dirs::home_dir()
        .map(|h| h.join("Library/LaunchAgents/com.shazam.tray.plist").exists())
        .unwrap_or(false)
}

fn toggle_autostart(menu_item: &MenuItem) {
    let home = dirs::home_dir().expect("No home dir");
    let plist_path = home.join("Library/LaunchAgents/com.shazam.tray.plist");

    if plist_path.exists() {
        let _ = std::process::Command::new("launchctl")
            .args(["unload", &plist_path.to_string_lossy()]).output();
        let _ = std::fs::remove_file(&plist_path);
        menu_item.set_text("  Start on Login");
    } else {
        let _ = std::fs::create_dir_all(home.join("Library/LaunchAgents"));
        let binary = std::env::current_exe().unwrap_or_else(|_| home.join("bin/shazam-tray"));
        let logs = home.join(".shazam/logs");
        let _ = std::fs::create_dir_all(&logs);

        let plist = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>com.shazam.tray</string>
    <key>ProgramArguments</key><array><string>{}</string></array>
    <key>RunAtLoad</key><true/>
    <key>KeepAlive</key><false/>
    <key>StandardOutPath</key><string>{}/tray.stdout.log</string>
    <key>StandardErrorPath</key><string>{}/tray.stderr.log</string>
</dict>
</plist>"#, binary.to_string_lossy(), logs.to_string_lossy(), logs.to_string_lossy());

        let _ = std::fs::write(&plist_path, plist);
        let _ = std::process::Command::new("launchctl")
            .args(["load", &plist_path.to_string_lossy()]).output();
        menu_item.set_text("✓ Start on Login");
    }
}
