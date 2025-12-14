use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::time::interval;
use tracing::{debug, error, info};

use crate::db::{custom_domains, models::DomainVerificationStatus, projects as db_projects};
use crate::server::state::ControllerState;

/// Domain verification loop - automatically verifies pending domains
pub struct DomainVerificationLoop {
    state: Arc<ControllerState>,
    check_interval: Duration,
}

impl DomainVerificationLoop {
    /// Create a new domain verification loop
    pub fn new(state: Arc<ControllerState>) -> Self {
        Self {
            state,
            check_interval: Duration::from_secs(300), // Check every 5 minutes
        }
    }

    /// Start the verification loop
    pub fn start(self: Arc<Self>) {
        tokio::spawn(async move {
            self.run().await;
        });
    }

    /// Main loop - processes pending domains and verifies them
    async fn run(&self) {
        info!(
            "Domain verification loop started (interval: {:?})",
            self.check_interval
        );
        let mut ticker = interval(self.check_interval);

        loop {
            ticker.tick().await;

            if let Err(e) = self.verify_pending_domains().await {
                error!("Error in domain verification loop: {}", e);
            }
        }
    }

    /// Verify all pending domains
    async fn verify_pending_domains(&self) -> Result<()> {
        // Query all domains with Pending status
        let pending_domains = sqlx::query_as!(
            crate::db::models::CustomDomain,
            r#"
            SELECT
                id, project_id, domain_name,
                verification_status as "verification_status: DomainVerificationStatus",
                verified_at,
                certificate_status as "certificate_status: crate::db::models::CertificateStatus",
                certificate_issued_at, certificate_expires_at,
                certificate_pem, certificate_key_pem, acme_order_url,
                created_at, updated_at
            FROM custom_domains
            WHERE verification_status = 'Pending'
            ORDER BY created_at ASC
            "#,
        )
        .fetch_all(&self.state.db_pool)
        .await?;

        if pending_domains.is_empty() {
            debug!("No pending domains to verify");
            return Ok(());
        }

        info!(
            "Found {} pending domain(s) to verify",
            pending_domains.len()
        );

        for domain in pending_domains {
            match self.verify_single_domain(&domain).await {
                Ok(verified) => {
                    if verified {
                        info!("✓ Domain {} verified successfully", domain.domain_name);
                    } else {
                        debug!(
                            "✗ Domain {} verification failed, will retry",
                            domain.domain_name
                        );
                    }
                }
                Err(e) => {
                    error!("Error verifying domain {}: {}", domain.domain_name, e);
                }
            }
        }

        Ok(())
    }

    /// Verify a single domain
    async fn verify_single_domain(&self, domain: &crate::db::models::CustomDomain) -> Result<bool> {
        // Get the project to compute CNAME target
        let project = db_projects::find_by_id(&self.state.db_pool, domain.project_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Project not found for domain"))?;

        // Compute CNAME target dynamically
        let cname_target = crate::server::domain::models::compute_cname_target(
            project.project_url.as_deref(),
            &project.name,
            "rise.dev", // TODO: Make this configurable from settings
        );

        // Perform DNS lookup
        let verification_result = verify_cname_async(&domain.domain_name, &cname_target).await;

        // Update domain status based on verification result
        let new_status = if verification_result {
            DomainVerificationStatus::Verified
        } else {
            // Keep as Pending for automatic retry, don't mark as Failed
            // User can manually verify if needed
            return Ok(false);
        };

        // Update domain verification status
        custom_domains::update_verification_status(&self.state.db_pool, domain.id, new_status)
            .await?;

        Ok(verification_result)
    }
}

/// Async DNS verification helper
async fn verify_cname_async(domain_name: &str, expected_target: &str) -> bool {
    use trust_dns_resolver::TokioAsyncResolver;

    // Create resolver
    let resolver = match TokioAsyncResolver::tokio_from_system_conf() {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to create DNS resolver: {}", e);
            return false;
        }
    };

    // Verify domain resolves to the same IPs as the target
    let domain_lookup = match resolver.lookup_ip(domain_name).await {
        Ok(lookup) => lookup,
        Err(e) => {
            debug!("DNS lookup failed for '{}': {}", domain_name, e);
            return false;
        }
    };

    let target_lookup = match resolver.lookup_ip(expected_target).await {
        Ok(ips) => ips,
        Err(e) => {
            error!("Failed to resolve target '{}': {}", expected_target, e);
            return false;
        }
    };

    // Compare IP addresses
    let domain_ips: Vec<_> = domain_lookup.iter().collect();
    let target_ips: Vec<_> = target_lookup.iter().collect();

    if domain_ips.is_empty() {
        debug!(
            "Domain '{}' does not resolve to any IP addresses",
            domain_name
        );
        return false;
    }

    // Check if at least one IP from domain matches target IPs
    let has_matching_ip = domain_ips.iter().any(|ip| target_ips.contains(ip));

    if has_matching_ip {
        debug!(
            "Domain '{}' resolves to same IPs as target '{}' - verified",
            domain_name, expected_target
        );
    } else {
        debug!(
            "Domain '{}' does not resolve to target '{}' - IPs don't match",
            domain_name, expected_target
        );
    }

    has_matching_ip
}
