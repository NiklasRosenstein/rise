use anyhow::{Context, Result};
use sqlx::PgPool;
use uuid::Uuid;

use crate::server::db::{models::TeamRole, teams};

/// Synchronize user's team memberships based on IdP groups claim
///
/// This function implements the complete IdP group synchronization algorithm:
/// 1. Creates teams that don't exist in Rise
/// 2. Marks teams as IdP-managed
/// 3. Updates team names to match IdP case (IdP is source of truth)
/// 4. Removes all owners when a team becomes IdP-managed
/// 5. Adds user to all groups in the IdP claim
/// 6. Removes user from IdP-managed teams NOT in the claim
///
/// All operations are performed within a database transaction for atomicity.
///
/// # Arguments
/// * `pool` - Database connection pool
/// * `user_id` - UUID of the user logging in
/// * `idp_groups` - List of group names from the IdP's "groups" claim
///
/// # Errors
/// Returns an error if database operations fail. The transaction will be rolled back.
pub async fn sync_user_groups(pool: &PgPool, user_id: Uuid, idp_groups: &[String]) -> Result<()> {
    // Start transaction for atomicity - all operations succeed or all fail
    let mut tx = pool.begin().await.context("Failed to start transaction")?;

    tracing::debug!(
        "Syncing {} IdP groups for user {}",
        idp_groups.len(),
        user_id
    );

    // Phase 1: Process each group in the IdP claim
    for group_name in idp_groups {
        // Find existing team (case-insensitive lookup)
        let existing_team = teams::find_by_name(&mut *tx, group_name)
            .await
            .context("Failed to find team by name")?;

        let team_id = if let Some(team) = existing_team {
            // Team exists - update if needed

            // Update name if case differs (IdP is authoritative source for casing)
            if team.name != *group_name {
                tracing::info!(
                    "Updating team name from '{}' to '{}' (IdP case correction)",
                    team.name,
                    group_name
                );
                teams::update_name(&mut *tx, team.id, group_name)
                    .await
                    .context("Failed to update team name")?;
            }

            // If team wasn't IdP-managed before, convert it
            if !team.idp_managed {
                tracing::info!(
                    "Converting team '{}' to IdP-managed (removing all owners)",
                    group_name
                );
                teams::set_idp_managed(&mut *tx, team.id, true)
                    .await
                    .context("Failed to mark team as IdP-managed")?;

                // Remove all owners when team becomes IdP-managed
                // This ensures IdP has full control over the team
                teams::remove_all_owners(&mut *tx, team.id)
                    .await
                    .context("Failed to remove owners")?;
            }

            team.id
        } else {
            // Team doesn't exist - create it as IdP-managed
            tracing::info!("Creating new IdP-managed team '{}'", group_name);
            let new_team = teams::create(&mut *tx, group_name)
                .await
                .context("Failed to create team")?;

            // Mark as IdP-managed immediately
            teams::set_idp_managed(&mut *tx, new_team.id, true)
                .await
                .context("Failed to mark new team as IdP-managed")?;

            new_team.id
        };

        // Add user as member if not already in the team
        let is_member = teams::is_member(&mut *tx, team_id, user_id)
            .await
            .context("Failed to check membership")?;

        if !is_member {
            tracing::debug!("Adding user to IdP-managed team '{}'", group_name);
            teams::add_member(&mut *tx, team_id, user_id, TeamRole::Member)
                .await
                .context("Failed to add user to team")?;
        }
    }

    // Phase 2: Remove user from IdP-managed teams NOT in the claim
    let all_idp_teams = teams::list_idp_managed(&mut *tx)
        .await
        .context("Failed to list IdP-managed teams")?;

    for team in all_idp_teams {
        // Case-insensitive check if team is in IdP groups claim
        let in_claim = idp_groups
            .iter()
            .any(|g| g.eq_ignore_ascii_case(&team.name));

        if !in_claim {
            // User should not be in this IdP-managed team
            let is_member = teams::is_member(&mut *tx, team.id, user_id)
                .await
                .context("Failed to check membership")?;

            if is_member {
                tracing::info!(
                    "Removing user from IdP-managed team '{}' (not in groups claim)",
                    team.name
                );
                teams::remove_member(&mut *tx, team.id, user_id)
                    .await
                    .context("Failed to remove user from team")?;
            }
        }
    }

    // Commit transaction - all changes are applied atomically
    tx.commit().await.context("Failed to commit transaction")?;

    tracing::debug!("Successfully synced IdP groups for user {}", user_id);

    Ok(())
}
