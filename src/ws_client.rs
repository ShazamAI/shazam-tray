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

        // Check if daemon is alive
        let alive = TcpStream::connect_timeout(
            &format!("127.0.0.1:{}", port).parse().unwrap(),
            Duration::from_secs(1),
        ).is_ok();

        if alive {
            if let Ok(mut s) = state.lock() {
                s.daemon_running = true;
            }

            // Fetch health info
            if let Some(health) = fetch_json(port, "/api/health") {
                if let Ok(mut s) = state.lock() {
                    // Extract companies from health response
                    if let Some(companies) = health.get("companies").and_then(|c| c.as_array()) {
                        // Update project list from active companies
                        let mut projects: Vec<ProjectStatus> = Vec::new();
                        for company in companies {
                            if let Some(name) = company.as_str() {
                                // Try to get detailed status for this company
                                let ws = health.get("workspace").and_then(|w| w.as_str()).unwrap_or("").to_string();
                                let existing = s.projects.iter().find(|p| p.name == name).cloned();
                                projects.push(existing.unwrap_or(ProjectStatus {
                                    name: name.to_string(),
                                    workspace: ws.clone(),
                                    agents_active: 0,
                                    agents_total: 0,
                                    tasks_pending: 0,
                                    tasks_running: 0,
                                    tasks_done: 0,
                                    total_cost: 0.0,
                                    git_branch: String::new(),
                                }));
                            }
                        }
                        s.projects = projects;
                    }
                }
            }

            // Fetch tasks to get counts per project
            if let Some(tasks_data) = fetch_json(port, "/api/tasks") {
                if let Some(tasks) = tasks_data.get("tasks").and_then(|t| t.as_array()) {
                    if let Ok(mut s) = state.lock() {
                        // Count tasks per company
                        for project in s.projects.iter_mut() {
                            let project_tasks: Vec<&serde_json::Value> = tasks.iter()
                                .filter(|t| t.get("company").and_then(|c| c.as_str()) == Some(&project.name))
                                .collect();

                            project.tasks_pending = project_tasks.iter()
                                .filter(|t| t.get("status").and_then(|s| s.as_str()) == Some("pending"))
                                .count() as u32;
                            project.tasks_running = project_tasks.iter()
                                .filter(|t| {
                                    let s = t.get("status").and_then(|s| s.as_str()).unwrap_or("");
                                    s == "in_progress" || s == "running"
                                })
                                .count() as u32;
                            project.tasks_done = project_tasks.iter()
                                .filter(|t| {
                                    let s = t.get("status").and_then(|s| s.as_str()).unwrap_or("");
                                    s == "completed" || s == "failed"
                                })
                                .count() as u32;
                        }
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

/// Simple HTTP GET — no external dependencies needed
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

    // Extract body after \r\n\r\n
    let body = response.split("\r\n\r\n").nth(1)?;

    // Handle chunked transfer encoding
    let json_str = if body.contains("\r\n") && body.chars().next()?.is_ascii_hexdigit() {
        // Chunked: first line is hex size, then data
        body.lines().nth(1).unwrap_or(body)
    } else {
        body
    };

    serde_json::from_str(json_str.trim()).ok()
}
