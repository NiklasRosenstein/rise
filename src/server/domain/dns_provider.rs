use async_trait::async_trait;
use anyhow::Result;

/// DNS provider trait for managing TXT records for ACME DNS-01 challenges
#[async_trait]
pub trait DnsProvider: Send + Sync {
    /// Create or update a TXT record for ACME DNS-01 challenge
    /// 
    /// # Arguments
    /// * `record_name` - The full DNS record name (e.g., "_acme-challenge.example.com")
    /// * `record_value` - The TXT record value from ACME challenge
    async fn create_txt_record(&self, record_name: &str, record_value: &str) -> Result<()>;

    /// Delete a TXT record
    /// 
    /// # Arguments
    /// * `record_name` - The full DNS record name to delete
    async fn delete_txt_record(&self, record_name: &str) -> Result<()>;

    /// Check if a TXT record exists and has the expected value
    /// 
    /// # Arguments
    /// * `record_name` - The full DNS record name
    /// * `expected_value` - The expected TXT record value
    async fn verify_txt_record(&self, record_name: &str, expected_value: &str) -> Result<bool>;
}

/// Cloudflare DNS provider implementation
#[cfg(feature = "server")]
pub struct CloudflareDnsProvider {
    api_token: String,
    zone_id: String,
}

#[cfg(feature = "server")]
impl CloudflareDnsProvider {
    pub fn new(api_token: String, zone_id: String) -> Self {
        Self {
            api_token,
            zone_id,
        }
    }

    /// Get the Cloudflare API client
    fn get_client(&self) -> Result<cloudflare::framework::async_api::Client> {
        let credentials = cloudflare::framework::auth::Credentials::UserAuthToken {
            token: self.api_token.clone(),
        };
        
        Ok(cloudflare::framework::async_api::Client::new(
            credentials,
            Default::default(),
            cloudflare::framework::Environment::Production,
        )?)
    }

    /// Extract the subdomain from a full record name for the configured zone
    /// For CNAME delegation: _acme-challenge.example.com -> _acme-challenge
    fn extract_record_name(&self, full_name: &str) -> String {
        // For CNAME delegation, we typically use the challenge subdomain
        // e.g., for example.com with CNAME at _acme-challenge.example.com,
        // we need to create TXT record at the delegated location
        
        // If the record already starts with the challenge prefix, use it as-is
        if full_name.starts_with("_acme-challenge") {
            full_name.split('.').next().unwrap_or(full_name).to_string()
        } else {
            format!("_acme-challenge.{}", full_name)
        }
    }
}

#[cfg(feature = "server")]
#[async_trait]
impl DnsProvider for CloudflareDnsProvider {
    async fn create_txt_record(&self, record_name: &str, record_value: &str) -> Result<()> {
        use cloudflare::endpoints::dns::{CreateDnsRecord, CreateDnsRecordParams, DnsContent};
        use cloudflare::framework::async_api::ApiClient;

        let client = self.get_client()?;
        let name = self.extract_record_name(record_name);

        let params = CreateDnsRecordParams {
            name: &name,
            content: DnsContent::TXT {
                content: record_value.to_string(),
            },
            ttl: Some(120), // Short TTL for challenges
            proxied: Some(false), // Don't proxy TXT records
            priority: None,
        };

        let zone_identifier = &cloudflare::framework::async_api::ApiClient::zone_identifier(&self.zone_id);
        
        client.request(&CreateDnsRecord {
            zone_identifier,
            params,
        }).await?;

        tracing::info!("Created TXT record {} = {}", name, record_value);
        Ok(())
    }

    async fn delete_txt_record(&self, record_name: &str) -> Result<()> {
        use cloudflare::endpoints::dns::{DeleteDnsRecord, ListDnsRecords, ListDnsRecordsParams};
        use cloudflare::framework::async_api::ApiClient;

        let client = self.get_client()?;
        let name = self.extract_record_name(record_name);

        let zone_identifier = &cloudflare::framework::async_api::ApiClient::zone_identifier(&self.zone_id);

        // First, list records to find the TXT record ID
        let list_params = ListDnsRecordsParams {
            name: Some(name.clone()),
            record_type: Some(cloudflare::endpoints::dns::DnsContent::TXT {
                content: String::new(),
            }.into()),
            ..Default::default()
        };

        let records = client.request(&ListDnsRecords {
            zone_identifier,
            params: list_params,
        }).await?;

        // Delete all matching TXT records
        for record in records.result {
            client.request(&DeleteDnsRecord {
                zone_identifier,
                identifier: &record.id,
            }).await?;
            tracing::info!("Deleted TXT record {}", name);
        }

        Ok(())
    }

    async fn verify_txt_record(&self, record_name: &str, expected_value: &str) -> Result<bool> {
        use cloudflare::endpoints::dns::{ListDnsRecords, ListDnsRecordsParams, DnsContent};
        use cloudflare::framework::async_api::ApiClient;

        let client = self.get_client()?;
        let name = self.extract_record_name(record_name);

        let zone_identifier = &cloudflare::framework::async_api::ApiClient::zone_identifier(&self.zone_id);

        let list_params = ListDnsRecordsParams {
            name: Some(name.clone()),
            record_type: Some(DnsContent::TXT {
                content: String::new(),
            }.into()),
            ..Default::default()
        };

        let records = client.request(&ListDnsRecords {
            zone_identifier,
            params: list_params,
        }).await?;

        // Check if any record matches the expected value
        for record in records.result {
            if let DnsContent::TXT { content } = record.content {
                if content == expected_value {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }
}

/// Create a DNS provider based on configuration
#[cfg(feature = "server")]
pub fn create_dns_provider(config: &crate::server::settings::DnsProviderConfig) -> Result<Box<dyn DnsProvider>> {
    match config {
        crate::server::settings::DnsProviderConfig::Cloudflare { api_token, zone_id } => {
            Ok(Box::new(CloudflareDnsProvider::new(
                api_token.clone(),
                zone_id.clone(),
            )))
        }
    }
}
