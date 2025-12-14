use anyhow::{Context, Result};
use instant_acme::{
    Account, ChallengeType, Identifier, LetsEncrypt, NewAccount, NewOrder, OrderStatus,
};
use rcgen::{CertificateParams, DistinguishedName, KeyPair};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use super::dns_provider::DnsProvider;
use crate::db::{acme_challenges, custom_domains};
use crate::server::encryption::EncryptionProvider;
use crate::server::settings::AcmeSettings;

/// ACME service for managing Let's Encrypt certificates with DNS-01 challenges
pub struct AcmeService {
    settings: AcmeSettings,
    dns_provider: Arc<dyn DnsProvider>,
    encryption_provider: Arc<dyn EncryptionProvider>,
    db_pool: sqlx::PgPool,
}

impl AcmeService {
    pub fn new(
        settings: AcmeSettings,
        dns_provider: Arc<dyn DnsProvider>,
        encryption_provider: Arc<dyn EncryptionProvider>,
        db_pool: sqlx::PgPool,
    ) -> Self {
        Self {
            settings,
            dns_provider,
            encryption_provider,
            db_pool,
        }
    }

    /// Initialize ACME account or load existing one
    async fn init_account(&self) -> Result<Account> {
        info!(
            "Initializing ACME account with {}",
            self.settings.directory_url
        );

        // For production use, we should persist account credentials to database
        // For now, create a new account each time (Let's Encrypt allows this)
        let directory_url = if self.settings.directory_url.contains("staging") {
            LetsEncrypt::Staging.url()
        } else {
            LetsEncrypt::Production.url()
        };

        let (account, _credentials) = Account::create(
            &NewAccount {
                contact: &[&format!("mailto:{}", self.settings.contact_email)],
                terms_of_service_agreed: true,
                only_return_existing: false,
            },
            directory_url,
            None,
        )
        .await?;

        info!("ACME account initialized successfully");
        Ok(account)
    }

    /// Request a certificate for a domain using DNS-01 challenge with CNAME delegation
    ///
    /// This implements CNAME delegation workflow:
    /// 1. User creates CNAME: _acme-challenge.example.com -> _acme-challenge-delegation.rise.dev
    /// 2. We create TXT record at _acme-challenge-delegation.rise.dev with the challenge value
    /// 3. ACME server validates by following the CNAME and checking the TXT record
    pub async fn request_certificate(
        &self,
        domain_id: uuid::Uuid,
        domain_name: &str,
    ) -> Result<()> {
        info!("Requesting certificate for domain: {}", domain_name);

        // Get the domain from database
        let domain = custom_domains::get_by_id(&self.db_pool, domain_id)
            .await?
            .context("Domain not found")?;

        // Verify domain is verified
        if domain.verification_status != crate::db::models::DomainVerificationStatus::Verified {
            anyhow::bail!("Domain must be verified before requesting certificate");
        }

        // Update certificate status to Pending
        custom_domains::update_certificate_status(
            &self.db_pool,
            domain_id,
            crate::db::models::CertificateStatus::Pending,
            None,
            None,
            None,
        )
        .await?;

        // Run certificate issuance in background to avoid blocking
        let service = self.clone_for_background();
        let domain_name_owned = domain_name.to_string();
        tokio::spawn(async move {
            if let Err(e) = service
                .issue_certificate_background(domain_id, domain_name_owned)
                .await
            {
                error!("Failed to issue certificate for {}: {}", domain_id, e);
                // Mark certificate as failed
                let _ = custom_domains::update_certificate_status(
                    &service.db_pool,
                    domain_id,
                    crate::db::models::CertificateStatus::Failed,
                    None,
                    None,
                    None,
                )
                .await;
            }
        });

        info!("ACME certificate request initiated for {}", domain_name);
        Ok(())
    }

    /// Clone necessary components for background task
    fn clone_for_background(&self) -> Self {
        Self {
            settings: self.settings.clone(),
            dns_provider: Arc::clone(&self.dns_provider),
            encryption_provider: Arc::clone(&self.encryption_provider),
            db_pool: self.db_pool.clone(),
        }
    }

    /// Background task for certificate issuance
    async fn issue_certificate_background(
        &self,
        domain_id: uuid::Uuid,
        domain_name: String,
    ) -> Result<()> {
        info!("Starting certificate issuance for {}", domain_name);

        // Step 1: Initialize ACME account
        let account = self.init_account().await?;

        // Step 2: Create new order for domain
        let identifier = Identifier::Dns(domain_name.clone());
        let mut order = account
            .new_order(&NewOrder {
                identifiers: &[identifier],
            })
            .await?;

        info!("Created ACME order for {}", domain_name);

        // Step 3: Get authorizations and find DNS-01 challenge
        let authorizations = order.authorizations().await?;
        let authorization = authorizations.first().context("No authorizations found")?;

        let challenge = authorization
            .challenges
            .iter()
            .find(|c| c.r#type == ChallengeType::Dns01)
            .context("No DNS-01 challenge found")?;

        let challenge_token = order.key_authorization(challenge).dns_value();

        info!(
            "Got DNS-01 challenge for {}: {}",
            domain_name, challenge_token
        );

        // Step 4: Determine TXT record name based on CNAME delegation setting
        let txt_record_name = if self.settings.cname_delegation {
            // With CNAME delegation: user creates CNAME _acme-challenge.example.com -> _acme-challenge-delegation.rise.dev
            // We create TXT record at the delegation target
            format!("_acme-challenge-delegation.{}", domain_name)
        } else {
            // Without delegation: create TXT record directly at _acme-challenge.example.com
            format!("_acme-challenge.{}", domain_name)
        };

        // Store challenge in database
        let db_challenge = acme_challenges::create(
            &self.db_pool,
            domain_id,
            crate::db::models::ChallengeType::Dns01,
            &txt_record_name,
            &challenge_token,
            Some(&challenge.url),
            None,
        )
        .await?;

        info!("Stored ACME challenge in database: {}", txt_record_name);

        // Step 5: Create TXT record via DNS provider
        self.dns_provider
            .create_txt_record(&txt_record_name, &challenge_token)
            .await
            .context("Failed to create TXT record")?;

        info!(
            "Created TXT record: {} = {}",
            txt_record_name, challenge_token
        );

        // Step 6: Wait for DNS propagation (30-60 seconds is typical)
        info!("Waiting for DNS propagation...");
        sleep(Duration::from_secs(45)).await;

        // Verify DNS record is reachable
        if let Err(e) = self
            .dns_provider
            .verify_txt_record(&txt_record_name, &challenge_token)
            .await
        {
            warn!("TXT record verification failed, but continuing: {}", e);
        }

        // Step 7: Notify ACME server that challenge is ready
        order.set_challenge_ready(&challenge.url).await?;
        info!("Notified ACME server that challenge is ready");

        // Step 8: Poll for order status
        let mut order_attempts = 0;
        loop {
            sleep(Duration::from_secs(5)).await;
            let state = order.refresh().await?;

            match state.status {
                OrderStatus::Ready => {
                    info!("Order is ready for finalization");
                    break;
                }
                OrderStatus::Invalid => {
                    anyhow::bail!("Order became invalid");
                }
                OrderStatus::Pending | OrderStatus::Processing => {
                    order_attempts += 1;
                    if order_attempts > 24 {
                        // 24 * 5 seconds = 2 minutes timeout
                        anyhow::bail!("Order validation timeout");
                    }
                    debug!("Order status: {:?}, waiting...", state.status);
                }
                OrderStatus::Valid => {
                    info!("Order is valid");
                    break;
                }
            }
        }

        // Step 9: Generate CSR and finalize order
        info!("Generating CSR for {}", domain_name);
        let mut params = CertificateParams::new(vec![domain_name.clone()])?;
        params.distinguished_name = DistinguishedName::new();
        let private_key = KeyPair::generate()?;
        let csr = params.serialize_request(&private_key)?;

        order.finalize(csr.der()).await?;
        info!("Finalized ACME order");

        // Step 10: Poll for certificate
        let mut cert_attempts = 0;
        let cert_chain_pem = loop {
            sleep(Duration::from_secs(2)).await;
            let state = order.refresh().await?;

            match state.status {
                OrderStatus::Valid => {
                    let pem = order
                        .certificate()
                        .await?
                        .context("Certificate not available")?;
                    info!("Downloaded certificate");
                    break pem;
                }
                OrderStatus::Invalid => {
                    anyhow::bail!("Order became invalid during certificate download");
                }
                OrderStatus::Processing => {
                    cert_attempts += 1;
                    if cert_attempts > 30 {
                        anyhow::bail!("Certificate download timeout");
                    }
                    debug!("Waiting for certificate...");
                }
                _ => {}
            }
        };

        // Step 11: Extract private key
        let private_key_pem = private_key.serialize_pem();

        // Step 12: Encrypt private key
        let encrypted_key = self
            .encryption_provider
            .encrypt(&private_key_pem)
            .await
            .context("Failed to encrypt private key")?;

        info!("Certificate issued successfully for {}", domain_name);

        // Step 13: Store certificate in database
        let issued_at = chrono::Utc::now();
        let expires_at = issued_at + chrono::Duration::days(90); // Let's Encrypt certs are 90 days

        custom_domains::update_certificate_status(
            &self.db_pool,
            domain_id,
            crate::db::models::CertificateStatus::Issued,
            Some(cert_chain_pem.as_str()),
            Some(encrypted_key.as_str()),
            Some(expires_at),
        )
        .await?;

        // Step 14: Clean up DNS record
        if let Err(e) = self.dns_provider.delete_txt_record(&txt_record_name).await {
            warn!("Failed to clean up TXT record {}: {}", txt_record_name, e);
        }

        // Mark challenge as valid in database
        acme_challenges::update_status(
            &self.db_pool,
            db_challenge.id,
            crate::db::models::ChallengeStatus::Valid,
        )
        .await?;

        info!("Certificate issuance complete for {}", domain_name);
        Ok(())
    }

    /// Renew a certificate that is expiring soon
    pub async fn renew_certificate(&self, domain_id: uuid::Uuid) -> Result<()> {
        let domain = custom_domains::get_by_id(&self.db_pool, domain_id)
            .await?
            .context("Domain not found")?;

        info!("Renewing certificate for {}", domain.domain_name);

        // Certificate renewal is the same as initial issuance
        self.request_certificate(domain_id, &domain.domain_name)
            .await
    }
}
