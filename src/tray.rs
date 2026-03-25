use crate::AppState;
use crate::daemon;
use crate::ws_client;
use std::sync::{Arc, Mutex};

use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIconBuilder,
};

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
            let b_val = (buf[i] as u16 + buf[i+1] as u16 + buf[i+2] as u16) / 3;
            let f = b_val as f32 / 255.0;
            buf[i]     = (r as f32 * f).min(255.0) as u8;
            buf[i + 1] = (g as f32 * f).min(255.0) as u8;
            buf[i + 2] = (b as f32 * f).min(255.0) as u8;
        }
    }
    tray_icon::Icon::from_rgba(buf, info.width, info.height).expect("Failed to create icon")
}

fn icon_default() -> tray_icon::Icon { build_icon_colored(255, 200, 0) }
fn icon_green() -> tray_icon::Icon   { build_icon_colored(0, 220, 80) }
fn icon_orange() -> tray_icon::Icon  { build_icon_colored(255, 160, 0) }
fn icon_gray() -> tray_icon::Icon    { build_icon_colored(120, 120, 120) }

#[derive(PartialEq, Clone)]
enum IconState { Offline, Idle, Running, ApprovalPending }

// ── Main ───────────────────────────────────────────────

pub fn run_tray(state: Arc<Mutex<AppState>>) {
    let mut event_loop = EventLoopBuilder::new().build();

    // Hide from Dock — menu bar only
    #[cfg(target_os = "macos")]
    {
        use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
        event_loop.set_activation_policy(ActivationPolicy::Accessory);
    }

    let menu = Menu::new();

    let status_item = MenuItem::new("⚡ Shazam — Starting...", false, None);
    let start_item = MenuItem::new("▶  Start Backend", true, None);
    let stop_item = MenuItem::new("■  Stop Backend", false, None);
    let restart_item = MenuItem::new("↻  Restart Backend", false, None);
    // Single project line (updates dynamically)
    let project_item = MenuItem::new("No active projects", false, None);
    let open_tui_item = MenuItem::new("Open TUI in Terminal", true, None);
    let notifications_item = MenuItem::new("✓ Notifications", true, None);
    let sounds_item = MenuItem::new("✓ Sounds", true, None);
    let autostart_item = MenuItem::new("  Start on Login", true, None);
    let quit_item = MenuItem::new("Quit Shazam Tray", true, None);

    let _ = menu.append(&status_item);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&start_item);
    let _ = menu.append(&stop_item);
    let _ = menu.append(&restart_item);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&project_item);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&open_tui_item);
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
    let project_id = project_item.id().clone();
    let open_tui_id = open_tui_item.id().clone();
    let notifications_id = notifications_item.id().clone();
    let sounds_id = sounds_item.id().clone();
    let autostart_id = autostart_item.id().clone();
    let quit_id = quit_item.id().clone();

    let mut last_update = std::time::Instant::now();
    let mut current_icon = IconState::Offline;
    let mut prev_approvals: u32 = 0;

    if is_autostart_installed() {
        autostart_item.set_text("✓ Start on Login");
    }

    event_loop.run(move |_event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(
            std::time::Instant::now() + std::time::Duration::from_secs(1),
        );

        // ── Clicks ───────────────────────────────────────
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
            if event.id == project_id {
                if let Ok(s) = state.lock() {
                    if let Some(p) = s.projects.first() {
                        let port = s.daemon_port;
                        let name = p.name.clone();
                        let workspace = p.workspace.clone();
                        let is_running = p.status == "running";
                        drop(s); // Release lock before blocking call

                        if is_running {
                            // Running → open TUI
                            open_tui_for_project(&workspace);
                        } else {
                            // Stopped → start the project
                            std::thread::spawn(move || {
                                ws_client::start_project(port, &name);
                            });
                        }
                    }
                }
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
        }

        // ── Periodic update ──────────────────────────────
        if last_update.elapsed() < std::time::Duration::from_secs(2) { return; }
        last_update = std::time::Instant::now();

        let Ok(s) = state.lock() else { return; };
        let running = s.daemon_running;

        start_item.set_enabled(!running);
        stop_item.set_enabled(running);
        restart_item.set_enabled(running);

        // Icon
        let new_icon = if !running { IconState::Offline }
        else if s.projects.is_empty() { IconState::Idle }
        else if s.projects.iter().any(|p| p.tasks_pending > 0) { IconState::ApprovalPending }
        else if s.projects.iter().any(|p| p.tasks_running > 0 || p.agents_active > 0) { IconState::Running }
        else { IconState::Idle };

        if new_icon != current_icon {
            let _ = _tray.set_icon(Some(match &new_icon {
                IconState::Offline => icon_gray(),
                IconState::Idle => icon_default(),
                IconState::Running => icon_green(),
                IconState::ApprovalPending => icon_orange(),
            }));
            current_icon = new_icon;
        }

        // Status text
        status_item.set_text(&if !running {
            "⚡ Shazam — Backend offline".to_string()
        } else if s.projects.is_empty() {
            "⚡ Shazam — Running (no projects)".to_string()
        } else {
            let a: u32 = s.projects.iter().map(|p| p.agents_active).sum();
            let t: u32 = s.projects.iter().map(|p| p.tasks_running).sum();
            let c: f64 = s.projects.iter().map(|p| p.total_cost).sum();
            format!("⚡ {} project(s) | {} agents | {} tasks | ${:.2}", s.projects.len(), a, t, c)
        });

        // Project item
        if s.projects.is_empty() {
            project_item.set_text("No projects registered");
            project_item.set_enabled(false);
        } else {
            let lines: Vec<String> = s.projects.iter().map(|p| {
                let icon = if p.status == "running" { "●" } else { "○" };
                if p.status == "running" {
                    format!("{} {} (running) — {}t/{}", icon, p.name, p.agents_active, p.agents_total)
                } else {
                    format!("{} {} (stopped) — click to start", icon, p.name)
                }
            }).collect();
            project_item.set_text(&lines.join("  |  "));
            project_item.set_enabled(true);
        }

        // Notifications
        let total_approvals: u32 = s.projects.iter().map(|p| p.tasks_pending).sum();
        if total_approvals > prev_approvals && prev_approvals > 0 {
            if s.notifications_enabled { send_notification("Shazam — Approval Needed", &format!("{} task(s) awaiting approval", total_approvals)); }
            if s.sounds_enabled { play_sound("Ping"); }
        }
        prev_approvals = total_approvals;

        let _ = _tray.set_tooltip(Some(&if s.projects.is_empty() {
            "Shazam — AI Agent Orchestrator".to_string()
        } else {
            s.projects.iter().map(|p| format!("{}: {}a {}t ${:.2}", p.name, p.agents_active, p.tasks_running, p.total_cost)).collect::<Vec<_>>().join("\n")
        }));
    });
}

// ── Helpers ────────────────────────────────────────────

fn open_tui_for_project(workspace: &str) {
    if workspace.is_empty() {
        let _ = std::process::Command::new("open").args(["-a", "Terminal"]).spawn();
        return;
    }
    let script = format!("tell application \"Terminal\" to do script \"cd {} && shazam\"", workspace.replace("\"", "\\\""));
    let _ = std::process::Command::new("osascript").args(["-e", &script]).spawn();
}

fn send_notification(title: &str, body: &str) {
    #[cfg(target_os = "macos")]
    { let _ = mac_notification_sys::Notification::default().title(title).message(body).send(); }
    #[cfg(target_os = "linux")]
    { let _ = notify_rust::Notification::new().summary(title).body(body).show(); }
    #[cfg(target_os = "windows")]
    { let _ = (title, body); }
}

fn play_sound(name: &str) {
    #[cfg(target_os = "macos")]
    { let _ = std::process::Command::new("afplay").args([&format!("/System/Library/Sounds/{}.aiff", name)]).spawn(); }
    #[cfg(not(target_os = "macos"))]
    { let _ = name; }
}

fn is_autostart_installed() -> bool {
    #[cfg(target_os = "macos")]
    { dirs::home_dir().map(|h| h.join("Library/LaunchAgents/com.shazam.tray.plist").exists()).unwrap_or(false) }
    #[cfg(not(target_os = "macos"))]
    { false }
}

fn toggle_autostart(menu_item: &MenuItem) {
    #[cfg(not(target_os = "macos"))]
    { let _ = menu_item; return; }

    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().expect("No home dir");
        let plist_path = home.join("Library/LaunchAgents/com.shazam.tray.plist");
        if plist_path.exists() {
            let _ = std::process::Command::new("launchctl").args(["unload", &plist_path.to_string_lossy()]).output();
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
            let _ = std::process::Command::new("launchctl").args(["load", &plist_path.to_string_lossy()]).output();
            menu_item.set_text("✓ Start on Login");
        }
    }
}
