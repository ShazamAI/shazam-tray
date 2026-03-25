mod daemon;
mod tray;
mod ws_client;
mod protocol;

use std::sync::{Arc, Mutex};

/// Shared state between tray UI and WebSocket connection
pub struct AppState {
    pub projects: Vec<ProjectStatus>,
    pub daemon_running: bool,
    pub daemon_port: u16,
    pub sounds_enabled: bool,
    pub notifications_enabled: bool,
}

#[derive(Clone, Debug)]
pub struct ProjectStatus {
    pub name: String,
    pub workspace: String,
    pub status: String, // "running" or "stopped"
    pub agents_active: u32,
    pub agents_total: u32,
    pub tasks_pending: u32,
    pub tasks_running: u32,
    pub tasks_done: u32,
    pub total_cost: f64,
    pub git_branch: String,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            projects: Vec::new(),
            daemon_running: false,
            daemon_port: 4040,
            sounds_enabled: true,
            notifications_enabled: true,
        }
    }
}

fn main() {
    // Initialize Sentry for error tracking
    let _sentry_guard = sentry::init(("https://1b3fbab3f097b65e9fb8b8c978383c2e@o4505191293779968.ingest.us.sentry.io/4511106667970560", sentry::ClientOptions {
        release: Some("shazam-tray@0.1.1".into()),
        environment: Some("production".into()),
        ..Default::default()
    }));

    let state = Arc::new(Mutex::new(AppState::default()));

    // Check if daemon is already running (don't auto-start)
    let state_clone = state.clone();
    std::thread::spawn(move || {
        daemon::check_daemon_status(state_clone);
    });

    // Start WebSocket listener for status updates
    let state_clone = state.clone();
    std::thread::spawn(move || {
        ws_client::connect_and_listen(state_clone);
    });

    // Run tray on main thread (required by macOS)
    tray::run_tray(state);
}
