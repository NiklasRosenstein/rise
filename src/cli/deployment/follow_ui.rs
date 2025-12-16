use anyhow::{bail, Result};
use reqwest::Client;
use serde::Deserialize;
use std::io::{self, IsTerminal, Write as _};
use std::time::{Duration, Instant};
use tracing::info;

use crate::api::models::{Deployment, DeploymentStatus};
use crate::config::Config;

use super::core::{fetch_deployment, parse_duration};

// Project info for fetching project URL
#[derive(Deserialize)]
struct ProjectInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    primary_url: Option<String>,
}

// Legacy Docker controller metadata structures (for backward compatibility with old deployments)
#[derive(Deserialize, Debug, Clone, PartialEq)]
struct DockerMetadata {
    #[serde(default)]
    reconcile_phase: ReconcilePhase,
    container_id: Option<String>,
    container_name: Option<String>,
    assigned_port: Option<u16>,
}

#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
enum ReconcilePhase {
    #[default]
    NotStarted,
    CreatingContainer,
    StartingContainer,
    WaitingForHealth,
    Completed,
}

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
    last_controller_phase: Option<ReconcilePhase>,
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
            last_controller_phase: None,
            last_error: None,
            last_url: None,
            last_metadata: serde_json::Value::Null,
            spinner_frame: 0,
            is_first_poll: true,
        }
    }

    fn should_log_state_change(
        &self,
        deployment: &Deployment,
        controller_phase: &Option<ReconcilePhase>,
    ) -> bool {
        self.is_first_poll
            || self.last_status != deployment.status
            || self.last_controller_phase != *controller_phase
    }

    fn update(&mut self, deployment: &Deployment, controller_phase: Option<ReconcilePhase>) {
        self.last_status = deployment.status.clone();
        self.last_controller_phase = controller_phase;
        self.last_error = deployment.error_message.clone();
        self.last_url = deployment.primary_url.clone();
        self.last_metadata = deployment.controller_metadata.clone();
        self.is_first_poll = false;
    }
}

/// Live status section that gets replaced on each poll
struct LiveStatusSection {
    pub last_line_count: usize,
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

    fn render(
        &mut self,
        deployment: &Deployment,
        state: &FollowState,
        controller_phase: &Option<ReconcilePhase>,
    ) -> String {
        // Clear previous output
        self.clear_previous();

        let mut output = String::new();
        let mut line_count = 0;

        // Status line with icon and color
        let icon = status_icon(&deployment.status);
        let color = status_color(&deployment.status);
        let spinner = if is_in_progress(&deployment.status) {
            format!("{} ", spinner_frame(state.spinner_frame))
        } else {
            String::new()
        };

        // Show deployment status + controller phase if available
        let status_text = if let Some(phase) = controller_phase {
            format!("{} ({})", deployment.status, format_controller_phase(phase))
        } else {
            format!("{}", deployment.status)
        };

        output.push_str(&format!(
            "{}{} Status:    {}{}{}\n",
            spinner,
            icon,
            color,
            status_text,
            ansi::RESET
        ));
        line_count += 1;

        // URL if available
        if let Some(ref url) = deployment.primary_url {
            output.push_str(&format!("   URL:       {}\n", url));
            line_count += 1;
        }

        // Error message if present
        if let Some(ref error) = deployment.error_message {
            output.push_str(&format!(
                "   {}Error:{} {}\n",
                "\x1B[31m",
                ansi::RESET,
                error
            ));
            line_count += 1;
        }

        // Controller metadata summary (container ID if available)
        if let Some(container_id) = extract_container_id(&deployment.controller_metadata) {
            output.push_str(&format!("   Container: {}\n", container_id));
            line_count += 1;
        }

        self.last_line_count = line_count;
        output
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

/// Parse controller metadata to extract deployment phase info (handles legacy Docker deployments)
fn parse_controller_metadata(metadata: &serde_json::Value) -> Option<DockerMetadata> {
    if metadata.is_null() || metadata == &serde_json::json!({}) {
        return None;
    }

    // Try to parse as Docker metadata (for legacy deployments)
    // For Kubernetes deployments, this will return None, which is fine
    serde_json::from_value::<DockerMetadata>(metadata.clone()).ok()
}

/// Extract container ID from metadata for display
fn extract_container_id(metadata: &serde_json::Value) -> Option<String> {
    parse_controller_metadata(metadata)
        .and_then(|m| m.container_id.map(|id| id[..12.min(id.len())].to_string()))
}

/// Format controller phase for display
fn format_controller_phase(phase: &ReconcilePhase) -> String {
    match phase {
        ReconcilePhase::NotStarted => "not started".to_string(),
        ReconcilePhase::CreatingContainer => "creating container".to_string(),
        ReconcilePhase::StartingContainer => "starting container".to_string(),
        ReconcilePhase::WaitingForHealth => "waiting for health".to_string(),
        ReconcilePhase::Completed => "running".to_string(),
    }
}

/// Log state change to tracing (appears in history)
fn log_state_change(
    project: &str,
    deployment_id: &str,
    status: &DeploymentStatus,
    controller_phase: &Option<ReconcilePhase>,
) {
    let status_text = if let Some(phase) = controller_phase {
        format!("{} ({})", status, format_controller_phase(phase))
    } else {
        format!("{}", status)
    };

    info!("Deployment {}:{} → {}", project, deployment_id, status_text);
}

/// Check if stdout is a TTY
fn is_tty() -> bool {
    io::stdout().is_terminal()
}

/// Print deployment snapshot (for non-follow mode)
pub fn print_deployment_snapshot(deployment: &Deployment) {
    // Parse controller metadata
    let controller_phase =
        parse_controller_metadata(&deployment.controller_metadata).map(|m| m.reconcile_phase);

    // Status line with icon and color
    let icon = status_icon(&deployment.status);
    let color = status_color(&deployment.status);

    // Show deployment status + controller phase if available
    let status_text = if let Some(phase) = controller_phase {
        format!(
            "{} ({})",
            deployment.status,
            format_controller_phase(&phase)
        )
    } else {
        format!("{}", deployment.status)
    };

    println!(
        "{} Status:         {}{}{}",
        icon,
        color,
        status_text,
        ansi::RESET
    );

    // Deployment ID
    println!("   Deployment ID:  {}", deployment.deployment_id);

    // Deployment group (if not default)
    if deployment.deployment_group != "default" {
        println!("   Group:          {}", deployment.deployment_group);
    }

    // Created by
    println!("   Created by:     {}", deployment.created_by_email);

    // Created/Updated timestamps
    println!("   Created:        {}", deployment.created);
    if deployment.updated != deployment.created {
        println!("   Updated:        {}", deployment.updated);
    }

    // Expires at (if set)
    if let Some(ref expires) = deployment.expires_at {
        println!("   Expires at:     {}", expires);
    }

    // Image and digest (if available)
    if let Some(ref image) = deployment.image {
        println!("   Image:          {}", image);
    }
    if let Some(ref digest) = deployment.image_digest {
        println!("   Image digest:   {}", digest);
    }

    // URL if available
    if let Some(ref url) = deployment.primary_url {
        println!("   URL:            {}", url);
    }

    // Controller metadata summary (container ID if available)
    if let Some(container_id) = extract_container_id(&deployment.controller_metadata) {
        println!("   Container:      {}", container_id);
    }

    // Error message if present
    if let Some(ref error) = deployment.error_message {
        println!("   \x1B[31mError:{} {}", ansi::RESET, error);
    }
}

/// Fetch project info to get project URL
async fn fetch_project_info(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    project: &str,
) -> Result<ProjectInfo> {
    let url = format!("{}/projects/{}", backend_url, project);

    let response = http_client.get(&url).bearer_auth(token).send().await?;

    if !response.status().is_success() {
        bail!("Failed to fetch project info");
    }

    let project_info: ProjectInfo = response.json().await?;
    Ok(project_info)
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
            &token,
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

    let result = async {
        loop {
            // Fetch deployment status
            let deployment =
                fetch_deployment(http_client, backend_url, &token, project, deployment_id).await?;

            // Parse controller metadata
            let controller_phase = parse_controller_metadata(&deployment.controller_metadata)
                .map(|m| m.reconcile_phase);

            // Check if this is a state change
            if state.should_log_state_change(&deployment, &controller_phase) {
                // Clear any existing live section before logging
                live_section.clear_previous();

                // Log state change to history
                log_state_change(
                    project,
                    deployment_id,
                    &deployment.status,
                    &controller_phase,
                );

                // Reset line count so next render works correctly
                live_section.last_line_count = 0;
            } else {
                // Only show live section when status is stable (spinner animation)
                let output = live_section.render(&deployment, &state, &controller_phase);
                print!("{}", output);
                io::stdout().flush().unwrap();
            }

            // Update state
            state.update(&deployment, controller_phase);
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

            // Wait before next poll (1 second for 2x faster spinner)
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
    .await;

    // Always show cursor again before returning
    print!("{}", ansi::SHOW_CURSOR);
    io::stdout().flush().unwrap();

    // Print project URL if deployment became active (Healthy in default group)
    if let Ok(ref deployment) = result {
        if deployment.status == DeploymentStatus::Healthy
            && deployment.deployment_group == "default"
        {
            if let Ok(project_info) =
                fetch_project_info(http_client, backend_url, &token, project).await
            {
                if let Some(url) = project_info.primary_url {
                    println!();
                    println!("Project URL: {}", url);
                }
            }
        }
    }

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
    let mut state = FollowState::new();

    loop {
        let deployment =
            fetch_deployment(http_client, backend_url, token, project, deployment_id).await?;

        // Parse controller metadata
        let controller_phase =
            parse_controller_metadata(&deployment.controller_metadata).map(|m| m.reconcile_phase);

        // Log state changes only (not every poll)
        if state.should_log_state_change(&deployment, &controller_phase) {
            log_state_change(
                project,
                deployment_id,
                &deployment.status,
                &controller_phase,
            );
        }

        // Update state
        state.update(&deployment, controller_phase);

        if is_terminal_state(&deployment.status) {
            // Print project URL if deployment became active (Healthy in default group)
            if deployment.status == DeploymentStatus::Healthy
                && deployment.deployment_group == "default"
            {
                if let Ok(project_info) =
                    fetch_project_info(http_client, backend_url, token, project).await
                {
                    if let Some(url) = project_info.primary_url {
                        println!();
                        println!("Project URL: {}", url);
                    }
                }
            }
            return Ok(deployment);
        }

        if start_time.elapsed() >= timeout {
            bail!("Timeout waiting for deployment");
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
