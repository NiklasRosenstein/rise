use anyhow::{Result, Context};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use crate::config::Config;

#[derive(Debug, Deserialize, Serialize)]
struct Team {
    id: String,
    name: String,
    members: Vec<String>,
    owners: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CreateTeamResponse {
    team: Team,
}

#[derive(Debug, Deserialize)]
struct UpdateTeamResponse {
    team: Team,
}

// Create a new team
pub async fn create_team(
    http_client: &Client,
    backend_url: &str,
    config: &Config,
    name: &str,
    owners: Vec<String>,
    members: Vec<String>,
) -> Result<()> {
    let token = config.get_token()
        .ok_or_else(|| anyhow::anyhow!("Not logged in. Please run 'rise login' first."))?;

    #[derive(Serialize)]
    struct CreateRequest {
        name: String,
        owners: Vec<String>,
        members: Vec<String>,
    }

    let request = CreateRequest {
        name: name.to_string(),
        owners,
        members,
    };

    let url = format!("{}/teams", backend_url);
    let response = http_client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&request)
        .send()
        .await
        .context("Failed to send create team request")?;

    if response.status().is_success() {
        let create_response: CreateTeamResponse = response.json().await
            .context("Failed to parse create team response")?;

        println!("✓ Team '{}' created successfully!", create_response.team.name);
        println!("  ID: {}", create_response.team.id);
        println!("  Owners: {}", create_response.team.owners.join(", "));
        println!("  Members: {}", create_response.team.members.join(", "));
    } else {
        let status = response.status();
        let error_text = response.text().await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!("Failed to create team (status {}): {}", status, error_text);
    }

    Ok(())
}

// List all teams
pub async fn list_teams(
    http_client: &Client,
    backend_url: &str,
    config: &Config,
) -> Result<()> {
    let token = config.get_token()
        .ok_or_else(|| anyhow::anyhow!("Not logged in. Please run 'rise login' first."))?;

    let url = format!("{}/teams", backend_url);
    let response = http_client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .context("Failed to send list teams request")?;

    if response.status().is_success() {
        let teams: Vec<Team> = response.json().await
            .context("Failed to parse list teams response")?;

        if teams.is_empty() {
            println!("No teams found.");
        } else {
            println!("Teams:");
            println!("{:<20} {:<36} {:<15} {:<15}", "NAME", "ID", "OWNERS", "MEMBERS");
            println!("{}", "-".repeat(90));
            for team in teams {
                println!("{:<20} {:<36} {:<15} {:<15}",
                    team.name,
                    team.id,
                    team.owners.len(),
                    team.members.len()
                );
            }
        }
    } else {
        let status = response.status();
        let error_text = response.text().await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!("Failed to list teams (status {}): {}", status, error_text);
    }

    Ok(())
}

// Show team details
pub async fn show_team(
    http_client: &Client,
    backend_url: &str,
    config: &Config,
    team_id: &str,
) -> Result<()> {
    let token = config.get_token()
        .ok_or_else(|| anyhow::anyhow!("Not logged in. Please run 'rise login' first."))?;

    let url = format!("{}/teams/{}", backend_url, team_id);
    let response = http_client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .context("Failed to send get team request")?;

    if response.status().is_success() {
        let team: Team = response.json().await
            .context("Failed to parse get team response")?;

        println!("Team: {}", team.name);
        println!("ID: {}", team.id);
        println!("\nOwners:");
        for owner in &team.owners {
            println!("  - {}", owner);
        }
        println!("\nMembers:");
        for member in &team.members {
            println!("  - {}", member);
        }
    } else {
        let status = response.status();
        let error_text = response.text().await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!("Failed to get team (status {}): {}", status, error_text);
    }

    Ok(())
}

// Update team (add/remove members and owners)
pub async fn update_team(
    http_client: &Client,
    backend_url: &str,
    config: &Config,
    team_id: &str,
    name: Option<String>,
    add_owners: Vec<String>,
    remove_owners: Vec<String>,
    add_members: Vec<String>,
    remove_members: Vec<String>,
) -> Result<()> {
    let token = config.get_token()
        .ok_or_else(|| anyhow::anyhow!("Not logged in. Please run 'rise login' first."))?;

    // First, get the current team state
    let get_url = format!("{}/teams/{}", backend_url, team_id);
    let get_response = http_client
        .get(&get_url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .context("Failed to get current team state")?;

    if !get_response.status().is_success() {
        anyhow::bail!("Team not found");
    }

    let mut team: Team = get_response.json().await
        .context("Failed to parse team response")?;

    // Apply changes
    for owner in add_owners {
        if !team.owners.contains(&owner) {
            team.owners.push(owner);
        }
    }
    for owner in remove_owners {
        team.owners.retain(|o| o != &owner);
    }
    for member in add_members {
        if !team.members.contains(&member) {
            team.members.push(member);
        }
    }
    for member in remove_members {
        team.members.retain(|m| m != &member);
    }

    // Update name if provided
    if let Some(new_name) = name {
        team.name = new_name;
    }

    #[derive(Serialize)]
    struct UpdateRequest {
        name: Option<String>,
        owners: Option<Vec<String>>,
        members: Option<Vec<String>>,
    }

    let request = UpdateRequest {
        name: Some(team.name.clone()),
        owners: Some(team.owners.clone()),
        members: Some(team.members.clone()),
    };

    let url = format!("{}/teams/{}", backend_url, team_id);
    let response = http_client
        .put(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&request)
        .send()
        .await
        .context("Failed to send update team request")?;

    if response.status().is_success() {
        let update_response: UpdateTeamResponse = response.json().await
            .context("Failed to parse update team response")?;

        println!("✓ Team '{}' updated successfully!", update_response.team.name);
        println!("  Owners: {}", update_response.team.owners.join(", "));
        println!("  Members: {}", update_response.team.members.join(", "));
    } else {
        let status = response.status();
        let error_text = response.text().await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!("Failed to update team (status {}): {}", status, error_text);
    }

    Ok(())
}

// Delete a team
pub async fn delete_team(
    http_client: &Client,
    backend_url: &str,
    config: &Config,
    team_id: &str,
) -> Result<()> {
    let token = config.get_token()
        .ok_or_else(|| anyhow::anyhow!("Not logged in. Please run 'rise login' first."))?;

    let url = format!("{}/teams/{}", backend_url, team_id);
    let response = http_client
        .delete(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .context("Failed to send delete team request")?;

    if response.status().is_success() {
        println!("✓ Team deleted successfully!");
    } else {
        let status = response.status();
        let error_text = response.text().await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!("Failed to delete team (status {}): {}", status, error_text);
    }

    Ok(())
}
