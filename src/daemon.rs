use crate::AppState;
use std::sync::{Arc, Mutex};
use std::process::Command;
use std::path::PathBuf;

const DAEMON_PID_FILE: &str = ".shazam/daemon.pid";
const DAEMON_PORT: u16 = 4040;

/// Just check if daemon is running (called on tray startup)
pub fn check_daemon_status(state: Arc<Mutex<AppState>>) {
    if is_daemon_alive() {
        if let Ok(mut s) = state.lock() {
            s.daemon_running = true;
            s.daemon_port = DAEMON_PORT;
        }
    }
}

/// Start the daemon and update state
pub fn ensure_daemon_running(state: Arc<Mutex<AppState>>) {
    if is_daemon_alive() {
        if let Ok(mut s) = state.lock() {
            s.daemon_running = true;
            s.daemon_port = DAEMON_PORT;
        }
        return;
    }

    match start_daemon() {
        Ok(_) => {
            if let Ok(mut s) = state.lock() {
                s.daemon_running = true;
                s.daemon_port = DAEMON_PORT;
            }
        }
        Err(e) => {
            eprintln!("Failed to start daemon: {}", e);
        }
    }
}

/// Check if daemon process is alive
fn is_daemon_alive() -> bool {
    // First check PID file
    let pid_path = daemon_pid_path();
    if let Ok(pid_str) = std::fs::read_to_string(&pid_path) {
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            let output = Command::new("kill")
                .args(["-0", &pid.to_string()])
                .output();
            if matches!(output, Ok(o) if o.status.success()) {
                return true;
            }
        }
    }

    // Fallback: check if port is responding
    check_port_alive(DAEMON_PORT)
}

/// Check if something is listening on the daemon port
fn check_port_alive(port: u16) -> bool {
    std::net::TcpStream::connect(format!("127.0.0.1:{}", port)).is_ok()
}

/// Start the daemon using `shazam daemon start` CLI command
fn start_daemon() -> Result<(), String> {
    // Find shazam-cli binary (shz, shazam, or shazam-cli)
    let shazam_bin = find_shazam_binary()
        .ok_or("shazam-cli not found. Install: https://shazam.dev")?;

    eprintln!("Starting daemon via {}...", shazam_bin.display());

    let output = Command::new(&shazam_bin)
        .args(["daemon", "start"])
        .output()
        .map_err(|e| format!("Failed to run shazam daemon start: {}", e))?;

    if output.status.success() {
        // Verify port is alive
        for _ in 0..10 {
            std::thread::sleep(std::time::Duration::from_millis(500));
            if check_port_alive(DAEMON_PORT) {
                return Ok(());
            }
        }
        Err("Daemon start command succeeded but port not responding".to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        Err(format!("shazam daemon start failed: {}{}", stdout, stderr))
    }
}

/// Stop the daemon
pub fn stop_daemon() {
    // Try via CLI first
    if let Some(bin) = find_shazam_binary() {
        let _ = Command::new(&bin)
            .args(["daemon", "stop"])
            .output();
        return;
    }

    // Fallback: kill by PID
    let pid_path = daemon_pid_path();
    if let Ok(pid_str) = std::fs::read_to_string(&pid_path) {
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            let _ = Command::new("kill")
                .args([&pid.to_string()])
                .output();
        }
    }
    let _ = std::fs::remove_file(&pid_path);
}

/// Find shazam-cli binary in PATH or known locations
fn find_shazam_binary() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let home_bin = home.join("bin");

    // Check ~/bin/ first (standard install location)
    for name in &["shazam-cli", "shazam", "shz"] {
        let path = home_bin.join(name);
        if path.is_file() {
            return Some(path);
        }
    }

    // Check PATH
    for name in &["shazam-cli", "shazam", "shz"] {
        if let Ok(output) = Command::new("which").arg(name).output() {
            if output.status.success() {
                let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path_str.is_empty() {
                    return Some(PathBuf::from(path_str));
                }
            }
        }
    }

    None
}

fn daemon_pid_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(DAEMON_PID_FILE)
}
