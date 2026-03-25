use crate::{AppState, ProjectStatus};
use std::sync::{Arc, Mutex};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

const POLL_INTERVAL_MS: u64 = 3000;

/// Poll the daemon's REST API for status updates
pub fn connect_and_listen(state: Arc<Mutex<AppState>>) {
    loop {
        let port = state.lock().map(|s| s.daemon_port).unwrap_or(4040);

        let alive = TcpStream::connect_timeout(
            &format!("127.0.0.1:{}", port).parse().unwrap(),
            Duration::from_secs(1),
        ).is_ok();

        if alive {
            if let Ok(mut s) = state.lock() {
                s.daemon_running = true;
            }

            // Fetch projects from registry (includes running + stopped)
            if let Some(data) = fetch_json(port, "/api/projects") {
                if let Some(projects) = data.get("projects").and_then(|p| p.as_array()) {
                    if let Ok(mut s) = state.lock() {
                        s.projects = projects.iter().filter_map(|p| {
                            let name = p.get("name")?.as_str()?.to_string();
                            let path = p.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let status = p.get("status").and_then(|v| v.as_str()).unwrap_or("stopped").to_string();
                            let agents_count = p.get("agents_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

                            Some(ProjectStatus {
                                name,
                                workspace: path,
                                status,
                                agents_active: 0,
                                agents_total: agents_count,
                                tasks_pending: 0,
                                tasks_running: 0,
                                tasks_done: 0,
                                total_cost: 0.0,
                                git_branch: String::new(),
                            })
                        }).collect();
                    }
                }
            }
        } else {
            if let Ok(mut s) = state.lock() {
                s.daemon_running = false;
                s.projects.clear();
            }
        }

        std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
    }
}

/// Simple HTTP GET
fn fetch_json(port: u16, path: &str) -> Option<serde_json::Value> {
    let addr = format!("127.0.0.1:{}", port);
    let mut stream = TcpStream::connect_timeout(
        &addr.parse().ok()?,
        Duration::from_secs(2),
    ).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;

    let request = format!(
        "GET {} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
        path, port
    );
    stream.write_all(request.as_bytes()).ok()?;

    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;

    let body = response.split("\r\n\r\n").nth(1)?;
    let json_str = if body.contains("\r\n") && body.chars().next()?.is_ascii_hexdigit() {
        body.lines().nth(1).unwrap_or(body)
    } else {
        body
    };
    serde_json::from_str(json_str.trim()).ok()
}

/// Start a project via REST API
pub fn start_project(port: u16, name: &str) -> bool {
    http_post(port, &format!("/api/projects/{}/start", name))
}

/// Stop a project via REST API
pub fn stop_project(port: u16, name: &str) -> bool {
    http_post(port, &format!("/api/projects/{}/stop", name))
}

fn http_post(port: u16, path: &str) -> bool {
    let addr = format!("127.0.0.1:{}", port);
    let Ok(mut stream) = TcpStream::connect_timeout(
        &addr.parse().unwrap(),
        Duration::from_secs(5),
    ) else { return false };

    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));

    let request = format!(
        "POST {} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        path, port
    );
    let _ = stream.write_all(request.as_bytes());

    let mut response = String::new();
    let _ = stream.read_to_string(&mut response);
    response.contains("200 OK")
}
