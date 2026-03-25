#![allow(dead_code)]
use serde::{Deserialize, Serialize};

/// Status message from the Shazam backend (same format as TUI protocol)
#[derive(Debug, Clone, Deserialize)]
pub struct StatusMsg {
    pub company: Option<String>,
    pub provider: Option<String>,
    pub total_cost: Option<f64>,
    pub agents_active: Option<u32>,
    pub agents_total: Option<u32>,
    pub budget_total: Option<u64>,
    pub budget_used: Option<u64>,
    pub git_branch: Option<String>,
    pub git_status: Option<String>,
    pub memory_mb: Option<u32>,
    pub tasks_awaiting: Option<u32>,
    pub tasks_done: Option<u32>,
    pub tasks_pending: Option<u32>,
    pub tasks_running: Option<u32>,
}

/// Event message from the backend
#[derive(Debug, Clone, Deserialize)]
pub struct EventMsg {
    pub agent: Option<String>,
    pub event: String,
    pub title: Option<String>,
    pub timestamp: Option<String>,
}

/// Inbound messages from backend WebSocket
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum InboundMsg {
    #[serde(rename = "status")]
    Status(StatusMsg),
    #[serde(rename = "event")]
    Event(EventMsg),
    #[serde(other)]
    Unknown,
}

/// Command to send to the backend
#[derive(Debug, Serialize)]
pub struct CommandMsg {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub raw: String,
}

impl CommandMsg {
    pub fn new(command: &str) -> Self {
        Self {
            msg_type: "command".to_string(),
            raw: command.to_string(),
        }
    }
}
