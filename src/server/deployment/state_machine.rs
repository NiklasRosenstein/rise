use crate::db::models::DeploymentStatus;
use anyhow::{bail, Result};

/// Check if a deployment status is terminal (no further transitions allowed)
pub fn is_terminal(status: &DeploymentStatus) -> bool {
    matches!(
        status,
        DeploymentStatus::Cancelled
            | DeploymentStatus::Stopped
            | DeploymentStatus::Superseded
            | DeploymentStatus::Failed
            | DeploymentStatus::Expired
    )
}

/// Check if a deployment is in an active running state
pub fn is_active(status: &DeploymentStatus) -> bool {
    matches!(
        status,
        DeploymentStatus::Healthy | DeploymentStatus::Unhealthy
    )
}

/// Check if a deployment can be cancelled
/// Only deployments in pre-infrastructure states can be cancelled
#[cfg_attr(not(test), allow(dead_code))]
pub fn is_cancellable(status: &DeploymentStatus) -> bool {
    matches!(
        status,
        DeploymentStatus::Pending
            | DeploymentStatus::Building
            | DeploymentStatus::Pushing
            | DeploymentStatus::Pushed
            | DeploymentStatus::Deploying
    )
}

/// Check if a deployment can be used as a rollback source
/// Only Healthy and Superseded deployments can be rolled back to
pub fn is_rollbackable(status: &DeploymentStatus) -> bool {
    matches!(
        status,
        DeploymentStatus::Healthy | DeploymentStatus::Superseded
    )
}

/// Check if a state transition is valid
pub fn is_valid_transition(from: &DeploymentStatus, to: &DeploymentStatus) -> bool {
    use DeploymentStatus::*;

    match (from, to) {
        // Can't transition from terminal states
        (from, _) if is_terminal(from) => false,

        // Pre-Infrastructure (Cancellation Path)
        (Pending | Building | Pushing | Pushed | Deploying, Cancelling) => true,
        (Cancelling, Cancelled) => true,

        // Build/Deploy Path
        (Pending, Building) => true,
        (Building, Pushing) => true,
        (Building, Pushed) => true, // Allow skipping Pushing state if status update fails
        (Pushing, Pushed) => true,
        (Pushed, Deploying) => true,

        // Deployment outcomes
        (Deploying, Healthy) => true, // Health checks pass
        (Deploying, Failed) => true,  // Health checks fail

        // Post-Infrastructure (Running State)
        (Healthy, Unhealthy) => true, // Health degradation
        (Unhealthy, Healthy) => true, // Health recovery
        (Unhealthy, Failed) => true,  // Timeout without recovery

        // Post-Infrastructure (Termination Path)
        (Healthy | Unhealthy, Terminating) => true,
        (Terminating, Stopped) => true, // User-initiated termination
        (Terminating, Superseded) => true, // Replaced by newer deployment
        (Terminating, Expired) => true, // Deployment expired

        // Build/Deploy failures (before reaching Healthy)
        (Pending | Building | Pushing | Pushed, Failed) => true,

        // All other transitions are invalid
        _ => false,
    }
}

/// Validate a state transition and return an error if invalid
pub fn validate_transition(from: &DeploymentStatus, to: &DeploymentStatus) -> Result<()> {
    if !is_valid_transition(from, to) {
        bail!(
            "Invalid deployment state transition from '{}' to '{}'",
            from,
            to
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use DeploymentStatus::*;

    #[test]
    fn test_terminal_states() {
        assert!(is_terminal(&Cancelled));
        assert!(is_terminal(&Stopped));
        assert!(is_terminal(&Superseded));
        assert!(is_terminal(&Failed));

        assert!(!is_terminal(&Pending));
        assert!(!is_terminal(&Healthy));
        assert!(!is_terminal(&Cancelling));
    }

    #[test]
    fn test_active_states() {
        assert!(is_active(&Healthy));
        assert!(is_active(&Unhealthy));

        assert!(!is_active(&Deploying));
        assert!(!is_active(&Failed));
    }

    #[test]
    fn test_cancellable_states() {
        assert!(is_cancellable(&Pending));
        assert!(is_cancellable(&Building));
        assert!(is_cancellable(&Pushing));
        assert!(is_cancellable(&Pushed));
        assert!(is_cancellable(&Deploying));

        assert!(!is_cancellable(&Healthy));
        assert!(!is_cancellable(&Unhealthy));
        assert!(!is_cancellable(&Cancelled));
    }

    #[test]
    fn test_terminable_states() {
        assert!(is_terminable(&Healthy));
        assert!(is_terminable(&Unhealthy));

        assert!(!is_terminable(&Deploying));
        assert!(!is_terminable(&Stopped));
    }

    #[test]
    fn test_valid_cancellation_path() {
        // Can cancel from pre-infrastructure states
        assert!(is_valid_transition(&Pending, &Cancelling));
        assert!(is_valid_transition(&Building, &Cancelling));
        assert!(is_valid_transition(&Deploying, &Cancelling));

        // Cancelling always succeeds to Cancelled
        assert!(is_valid_transition(&Cancelling, &Cancelled));

        // Cannot transition from Cancelling to Failed
        assert!(!is_valid_transition(&Cancelling, &Failed));
    }

    #[test]
    fn test_valid_termination_path() {
        // Can terminate from post-infrastructure states
        assert!(is_valid_transition(&Healthy, &Terminating));
        assert!(is_valid_transition(&Unhealthy, &Terminating));

        // Terminating succeeds to Stopped or Superseded
        assert!(is_valid_transition(&Terminating, &Stopped));
        assert!(is_valid_transition(&Terminating, &Superseded));

        // Cannot transition from Terminating to Failed
        assert!(!is_valid_transition(&Terminating, &Failed));
    }

    #[test]
    fn test_healthy_unhealthy_cannot_be_cancelled() {
        // Healthy/Unhealthy cannot go to Cancelled
        assert!(!is_valid_transition(&Healthy, &Cancelled));
        assert!(!is_valid_transition(&Unhealthy, &Cancelled));

        // They must use Terminating
        assert!(is_valid_transition(&Healthy, &Terminating));
        assert!(is_valid_transition(&Unhealthy, &Terminating));
    }

    #[test]
    fn test_deployment_path() {
        // Normal deployment flow
        assert!(is_valid_transition(&Pending, &Building));
        assert!(is_valid_transition(&Building, &Pushing));
        assert!(is_valid_transition(&Pushing, &Pushed));
        assert!(is_valid_transition(&Pushed, &Deploying));
        assert!(is_valid_transition(&Deploying, &Healthy));

        // Allow skipping Pushing if status update fails
        assert!(is_valid_transition(&Building, &Pushed));

        // Health state transitions
        assert!(is_valid_transition(&Healthy, &Unhealthy));
        assert!(is_valid_transition(&Unhealthy, &Healthy));
        assert!(is_valid_transition(&Unhealthy, &Failed));
    }

    #[test]
    fn test_terminal_states_no_transitions() {
        // Cannot transition from terminal states
        assert!(!is_valid_transition(&Cancelled, &Pending));
        assert!(!is_valid_transition(&Stopped, &Healthy));
        assert!(!is_valid_transition(&Superseded, &Deploying));
        assert!(!is_valid_transition(&Failed, &Healthy));
    }

    #[test]
    fn test_invalid_transitions() {
        // Cannot skip states in deployment path
        assert!(!is_valid_transition(&Pending, &Deploying));
        assert!(!is_valid_transition(&Building, &Healthy));

        // Cannot go back in deployment path
        assert!(!is_valid_transition(&Deploying, &Building));
        assert!(!is_valid_transition(&Healthy, &Pending));
    }
}
