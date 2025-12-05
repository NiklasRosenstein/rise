use anyhow::{Context, Result, bail};
use reqwest::Client;
use std::io::{self, IsTerminal, Write as _};
use std::time::{Duration, Instant};
use tracing::info;

use crate::config::Config;
use rise_backend::deployment::models::{Deployment, DeploymentStatus};

use super::core::{fetch_deployment, parse_duration};

// ANSI escape codes for terminal manipulation
mod ansi {
    pub const CLEAR_LINE: &str = "\x1B[2K";
    pub const HIDE_CURSOR: &str = "\x1B[?25l";
    pub const SHOW_CURSOR: &str = "\x1B[?25h";
    pub const RESET: &str = "\x1B[0m";

    /// Move cursor up n lines
    pub fn move_up(n: usize) -> String {
        format!("\x1B[{}A", n)
    }

    /// Move cursor to beginning of line
    pub const CURSOR_TO_START: &str = "\r";
}

// Spinner animation frames
const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// State tracking between polls
struct FollowState {
    last_status: DeploymentStatus,
    last_error: Option<String>,
    last_url: Option<String>,
    last_metadata: serde_json::Value,
    spinner_frame: usize,
    is_first_poll: bool,
}

impl FollowState {
    fn new() -> Self {
        Self {
            last_status: DeploymentStatus::Pending,
            last_error: None,
            last_url: None,
            last_metadata: serde_json::Value::Null,
            spinner_frame: 0,
            is_first_poll: true,
        }
    }

    fn should_log_state_change(&self, deployment: &Deployment) -> bool {
        self.is_first_poll || self.last_status != deployment.status
    }

    fn update(&mut self, deployment: &Deployment) {
        self.last_status = deployment.status.clone();
        self.last_error = deployment.error_message.clone();
        self.last_url = deployment.deployment_url.clone();
        self.last_metadata = deployment.controller_metadata.clone();
        self.is_first_poll = false;
    }
}

/// Live status section that gets replaced on each poll
struct LiveStatusSection {
    last_line_count: usize,
}

impl LiveStatusSection {
    fn new() -> Self {
        Self { last_line_count: 0 }
    }

    fn clear_previous(&self) {
        if self.last_line_count > 0 {
            // Move cursor up and clear each line
            for _ in 0..self.last_line_count {
                print!(
                    "{}{}{}",
                    ansi::move_up(1),
                    ansi::CURSOR_TO_START,
                    ansi::CLEAR_LINE
                );
            }
            print!("{}", ansi::CURSOR_TO_START);
            io::stdout().flush().unwrap();
        }
    }

    fn render(&mut self, deployment: &Deployment, state: &FollowState) -> String {
        // Clear previous output
        self.clear_previous();

        let mut output = String::new();
        let mut line_count = 0;

        // Separator
        output.push_str(&format_separator("LIVE STATUS"));
        line_count += 1;

        // Status line with icon and color
        let icon = status_icon(&deployment.status);
        let color = status_color(&deployment.status);
        let spinner = if is_in_progress(&deployment.status) {
            format!("{} ", spinner_frame(state.spinner_frame))
        } else {
            String::new()
        };

        output.push_str(&format!(
            "{}{} Status:    {}{}{}\n",
            spinner,
            icon,
            color,
            deployment.status,
            ansi::RESET
        ));
        line_count += 1;

        // URL if available
        if let Some(ref url) = deployment.deployment_url {
            output.push_str(&format!("   URL:       {}\n", url));
            line_count += 1;
        }

        // Updated timestamp
        output.push_str(&format!("   Updated:   {}\n", deployment.updated));
        line_count += 1;

        // Error message if present
        if let Some(ref error) = deployment.error_message {
            output.push_str(&format!(
                "\n   {}Error:{} {}\n",
                "\x1B[31m",
                ansi::RESET,
                error
            ));
            line_count += 2;
        }

        // Controller metadata summary
        if let Some(metadata_summary) = format_metadata_summary(&deployment.controller_metadata) {
            output.push_str(&format!("\n   Controller: {}\n", metadata_summary));
            line_count += 2;
        }

        // Bottom separator
        output.push_str(&format_separator(""));
        line_count += 1;

        self.last_line_count = line_count;
        output
    }
}

/// Format a horizontal separator line
fn format_separator(title: &str) -> String {
    let separator = "━".repeat(50);
    if title.is_empty() {
        format!("{}\n", separator)
    } else {
        format!("{}\n", separator)
    }
}

/// Get status color ANSI code
fn status_color(status: &DeploymentStatus) -> &'static str {
    match status {
        DeploymentStatus::Healthy => "\x1B[32m",   // Green
        DeploymentStatus::Failed => "\x1B[31m",    // Red
        DeploymentStatus::Deploying => "\x1B[33m", // Yellow
        DeploymentStatus::Building => "\x1B[36m",  // Cyan
        DeploymentStatus::Pushing => "\x1B[36m",   // Cyan
        DeploymentStatus::Unhealthy => "\x1B[31m", // Red
        DeploymentStatus::Cancelled => "\x1B[90m", // Gray
        DeploymentStatus::Stopped => "\x1B[90m",   // Gray
        _ => "\x1B[37m",                           // White
    }
}

/// Get status icon
fn status_icon(status: &DeploymentStatus) -> &'static str {
    match status {
        DeploymentStatus::Healthy => "✓",
        DeploymentStatus::Failed => "✗",
        DeploymentStatus::Deploying => "⚙",
        DeploymentStatus::Building => "🔨",
        DeploymentStatus::Pushing => "⬆",
        DeploymentStatus::Pushed => "✓",
        DeploymentStatus::Unhealthy => "⚠",
        DeploymentStatus::Cancelled => "⊘",
        DeploymentStatus::Cancelling => "⊘",
        DeploymentStatus::Terminating => "⊘",
        DeploymentStatus::Stopped => "■",
        DeploymentStatus::Superseded => "↻",
        DeploymentStatus::Expired => "⏱",
        DeploymentStatus::Pending => "○",
    }
}

/// Get spinner frame
fn spinner_frame(frame_num: usize) -> &'static str {
    SPINNER_FRAMES[frame_num % SPINNER_FRAMES.len()]
}

/// Check if status is in-progress (should show spinner)
fn is_in_progress(status: &DeploymentStatus) -> bool {
    matches!(
        status,
        DeploymentStatus::Pending
            | DeploymentStatus::Building
            | DeploymentStatus::Pushing
            | DeploymentStatus::Pushed
            | DeploymentStatus::Deploying
            | DeploymentStatus::Cancelling
            | DeploymentStatus::Terminating
    )
}

/// Check if status is terminal
fn is_terminal_state(status: &DeploymentStatus) -> bool {
    matches!(
        status,
        DeploymentStatus::Healthy
            | DeploymentStatus::Failed
            | DeploymentStatus::Cancelled
            | DeploymentStatus::Stopped
            | DeploymentStatus::Superseded
            | DeploymentStatus::Expired
    )
}

/// Format controller metadata as compact summary
fn format_metadata_summary(metadata: &serde_json::Value) -> Option<String> {
    if metadata.is_null() || metadata == &serde_json::json!({}) {
        return None;
    }

    if let Some(obj) = metadata.as_object() {
        let mut parts = Vec::new();

        if let Some(container_id) = obj.get("container_id").and_then(|v| v.as_str()) {
            let short_id = &container_id[..12.min(container_id.len())];
            parts.push(format!("Container {}", short_id));
        }

        if let Some(status) = obj.get("container_status").and_then(|v| v.as_str()) {
            parts.push(status.to_string());
        }

        if !parts.is_empty() {
            return Some(parts.join(" | "));
        }
    }

    None
}

/// Log state change to tracing (appears in history)
fn log_state_change(project: &str, deployment_id: &str, status: &DeploymentStatus) {
    info!("Deployment {}:{} → {}", project, deployment_id, status);
}

/// Check if stdout is a TTY
fn is_tty() -> bool {
    io::stdout().is_terminal()
}

/// Main follow function with enhanced UX
pub async fn follow_deployment_with_ui(
    http_client: &Client,
    backend_url: &str,
    config: &Config,
    project: &str,
    deployment_id: &str,
    timeout_str: &str,
) -> Result<Deployment> {
    let token = config
        .get_token()
        .ok_or_else(|| anyhow::anyhow!("Not authenticated"))?;

    let timeout = parse_duration(timeout_str)?;
    let start_time = Instant::now();

    // Check if we're in a TTY - if not, fall back to simple mode
    if !is_tty() {
        return follow_deployment_simple(
            http_client,
            backend_url,
            token,
            project,
            deployment_id,
            timeout,
        )
        .await;
    }

    let mut state = FollowState::new();
    let mut live_section = LiveStatusSection::new();

    // Hide cursor for cleaner output
    print!("{}", ansi::HIDE_CURSOR);
    io::stdout().flush().unwrap();

    // Print initial separator for history section
    println!("{}", format_separator("STATE CHANGE HISTORY"));

    let result = async {
        loop {
            // Fetch deployment status
            let deployment =
                fetch_deployment(http_client, backend_url, token, project, deployment_id).await?;

            // Log state changes to history
            if state.should_log_state_change(&deployment) {
                log_state_change(project, deployment_id, &deployment.status);
            }

            // Render live status section
            let output = live_section.render(&deployment, &state);
            print!("{}", output);
            io::stdout().flush().unwrap();

            // Update state
            state.update(&deployment);
            state.spinner_frame = (state.spinner_frame + 1) % SPINNER_FRAMES.len();

            // Check if deployment reached terminal state
            if is_terminal_state(&deployment.status) {
                return Ok(deployment);
            }

            // Check timeout
            if start_time.elapsed() >= timeout {
                bail!(
                    "Timeout waiting for deployment to complete after {:?}",
                    timeout
                );
            }

            // Wait before next poll
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
    .await;

    // Always show cursor again before returning
    print!("{}", ansi::SHOW_CURSOR);
    io::stdout().flush().unwrap();

    result
}

/// Simple fallback for non-TTY environments (pipes, redirects)
async fn follow_deployment_simple(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    project: &str,
    deployment_id: &str,
    timeout: Duration,
) -> Result<Deployment> {
    let start_time = Instant::now();

    loop {
        let deployment =
            fetch_deployment(http_client, backend_url, token, project, deployment_id).await?;

        // Simple logging for non-TTY
        info!(
            "Deployment {}:{} status: {}",
            project, deployment_id, deployment.status
        );

        if is_terminal_state(&deployment.status) {
            return Ok(deployment);
        }

        if start_time.elapsed() >= timeout {
            bail!("Timeout waiting for deployment");
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
