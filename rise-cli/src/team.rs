use anyhow::{Result, Context};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use crate::config::Config;

#[derive(Debug, Deserialize)]
struct MeResponse {
    id: String,
    email: String,
}

#[derive(Debug, Serialize)]
struct UsersLookupRequest {
    emails: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct UsersLookupResponse {
    users: Vec<UserInfo>,
}

#[derive(Debug, Deserialize)]
struct UserInfo {
    id: String,
    email: String,
}

// Helper function to lookup user IDs from emails
async fn lookup_users(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    emails: Vec<String>,
) -> Result<Vec<String>> {
    if emails.is_empty() {
        return Ok(Vec::new());
    }

    let url = format!("{}/users/lookup", backend_url);
    let request = UsersLookupRequest { emails: emails.clone() };

    let response = http_client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&request)
        .send()
        .await
        .context("Failed to lookup users")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!("Failed to lookup users (status {}): {}", status, error_text);
    }

    let lookup_response: UsersLookupResponse = response.json().await
        .context("Failed to parse lookup response")?;

    Ok(lookup_response.users.into_iter().map(|u| u.id).collect())
}

// Helper function to get current user info
async fn get_current_user(
    http_client: &Client,
    backend_url: &str,
    token: &str,
) -> Result<MeResponse> {
    let url = format!("{}/me", backend_url);

    let response = http_client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .context("Failed to get current user")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!("Failed to get current user (status {}): {}", status, error_text);
    }

    let me_response: MeResponse = response.json().await
        .context("Failed to parse me response")?;

    Ok(me_response)
}

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
    owners: Option<Vec<String>>,
    members: Vec<String>,
) -> Result<()> {
    let token = config.get_token()
        .ok_or_else(|| anyhow::anyhow!("Not logged in. Please run 'rise login' first."))?;

    // If no owners specified, use current user
    let owner_emails = if let Some(emails) = owners {
        emails
    } else {
        let current_user = get_current_user(http_client, backend_url, token).await?;
        vec![current_user.email]
    };

    // Convert email addresses to user IDs
    let owner_ids = lookup_users(http_client, backend_url, token, owner_emails).await?;
    let member_ids = lookup_users(http_client, backend_url, token, members).await?;

    #[derive(Serialize)]
    struct CreateRequest {
        name: String,
        owners: Vec<String>,
        members: Vec<String>,
    }

    let request = CreateRequest {
        name: name.to_string(),
        owners: owner_ids,
        members: member_ids,
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

    // Convert email addresses to user IDs
    let add_owner_ids = lookup_users(http_client, backend_url, token, add_owners).await?;
    let remove_owner_ids = lookup_users(http_client, backend_url, token, remove_owners).await?;
    let add_member_ids = lookup_users(http_client, backend_url, token, add_members).await?;
    let remove_member_ids = lookup_users(http_client, backend_url, token, remove_members).await?;

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
    for owner in add_owner_ids {
        if !team.owners.contains(&owner) {
            team.owners.push(owner);
        }
    }
    for owner in remove_owner_ids {
        team.owners.retain(|o| o != &owner);
    }
    for member in add_member_ids {
        if !team.members.contains(&member) {
            team.members.push(member);
        }
    }
    for member in remove_member_ids {
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
