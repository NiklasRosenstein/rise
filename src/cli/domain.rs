use crate::config::Config;
use anyhow::{Context, Result};
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Attribute, Cell, Table};
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
struct CustomDomain {
    id: String,
    project_id: String,
    domain_name: String,
    cname_target: String,
    verification_status: String,
    verified_at: Option<String>,
    certificate_status: String,
    certificate_issued_at: Option<String>,
    certificate_expires_at: Option<String>,
    created: String,
    updated: String,
}

#[derive(Debug, Deserialize)]
struct AddDomainResponse {
    domain: CustomDomain,
    instructions: DomainSetupInstructions,
}

#[derive(Debug, Deserialize)]
struct DomainSetupInstructions {
    cname_record: CnameRecord,
    message: String,
}

#[derive(Debug, Deserialize)]
struct CnameRecord {
    name: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct VerifyDomainResponse {
    domain: CustomDomain,
    verification_result: VerificationResult,
}

#[derive(Debug, Deserialize)]
struct VerificationResult {
    success: bool,
    message: String,
    expected_value: Option<String>,
    actual_value: Option<String>,
}

/// Add a custom domain to a project
pub async fn add_domain(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    project: &str,
    domain_name: &str,
) -> Result<()> {
    let url = format!("{}/projects/{}/domains", backend_url, project);

    let payload = serde_json::json!({
        "domain_name": domain_name,
    });

    let response = http_client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&payload)
        .send()
        .await
        .context("Failed to add domain")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!("Failed to add domain (status {}): {}", status, error_text);
    }

    let add_response: AddDomainResponse = response
        .json()
        .await
        .context("Failed to parse add domain response")?;

    println!("✅ Custom domain added successfully!");
    println!();
    println!("Domain: {}", add_response.domain.domain_name);
    println!("Status: {}", add_response.domain.verification_status);
    println!();
    println!("📋 Next steps:");
    println!();
    println!("1. Configure a CNAME record for your domain:");
    println!("   Name:  {}", add_response.instructions.cname_record.name);
    println!("   Value: {}", add_response.instructions.cname_record.value);
    println!();
    println!("2. Wait for DNS propagation (this can take a few minutes)");
    println!();
    println!("3. Verify the domain configuration:");
    println!("   rise domain verify {} {}", project, domain_name);
    println!();

    Ok(())
}

/// List custom domains for a project
pub async fn list_domains(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    project: &str,
) -> Result<()> {
    let url = format!("{}/projects/{}/domains", backend_url, project);

    let response = http_client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .context("Failed to list domains")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!("Failed to list domains (status {}): {}", status, error_text);
    }

    let domains: Vec<CustomDomain> = response
        .json()
        .await
        .context("Failed to parse domains response")?;

    if domains.is_empty() {
        println!("No custom domains configured for project '{}'", project);
        println!();
        println!("Add a custom domain with:");
        println!("  rise domain add {} <domain-name>", project);
        return Ok(());
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Domain").add_attribute(Attribute::Bold),
            Cell::new("CNAME Target").add_attribute(Attribute::Bold),
            Cell::new("Verification").add_attribute(Attribute::Bold),
            Cell::new("Certificate").add_attribute(Attribute::Bold),
            Cell::new("Created").add_attribute(Attribute::Bold),
        ]);

    for domain in domains {
        let verification_status = match domain.verification_status.as_str() {
            "Verified" => format!("✅ {}", domain.verification_status),
            "Failed" => format!("❌ {}", domain.verification_status),
            _ => format!("⏳ {}", domain.verification_status),
        };

        let cert_status = match domain.certificate_status.as_str() {
            "Issued" => format!("✅ {}", domain.certificate_status),
            "Failed" => format!("❌ {}", domain.certificate_status),
            "Pending" => format!("⏳ {}", domain.certificate_status),
            _ => domain.certificate_status.clone(),
        };

        let created = domain.created.split('T').next().unwrap_or(&domain.created);

        table.add_row(vec![
            domain.domain_name,
            domain.cname_target,
            verification_status,
            cert_status,
            created.to_string(),
        ]);
    }

    println!("{}", table);
    Ok(())
}

/// Delete a custom domain
pub async fn delete_domain(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    project: &str,
    domain_name: &str,
) -> Result<()> {
    let url = format!("{}/projects/{}/domains/{}", backend_url, project, domain_name);

    let response = http_client
        .delete(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .context("Failed to delete domain")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!("Failed to delete domain (status {}): {}", status, error_text);
    }

    println!("✅ Domain '{}' deleted successfully", domain_name);
    Ok(())
}

/// Verify a custom domain's CNAME configuration
pub async fn verify_domain(
    http_client: &Client,
    backend_url: &str,
    token: &str,
    project: &str,
    domain_name: &str,
) -> Result<()> {
    let url = format!(
        "{}/projects/{}/domains/{}/verify",
        backend_url, project, domain_name
    );

    println!("🔍 Verifying domain configuration...");

    let response = http_client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .context("Failed to verify domain")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        anyhow::bail!("Failed to verify domain (status {}): {}", status, error_text);
    }

    let verify_response: VerifyDomainResponse = response
        .json()
        .await
        .context("Failed to parse verify response")?;

    println!();
    if verify_response.verification_result.success {
        println!("✅ Domain verification successful!");
        println!();
        println!("Domain: {}", verify_response.domain.domain_name);
        println!("Status: {}", verify_response.domain.verification_status);
        println!("Message: {}", verify_response.verification_result.message);
        if let Some(verified_at) = verify_response.domain.verified_at {
            println!("Verified at: {}", verified_at);
        }
    } else {
        println!("❌ Domain verification failed");
        println!();
        println!("Domain: {}", verify_response.domain.domain_name);
        println!("Message: {}", verify_response.verification_result.message);
        if let Some(expected) = verify_response.verification_result.expected_value {
            println!("Expected: {}", expected);
        }
        if let Some(actual) = verify_response.verification_result.actual_value {
            println!("Actual: {}", actual);
        }
        println!();
        println!("Please check your DNS configuration and try again.");
    }

    Ok(())
}

/// Execute domain subcommands
pub async fn handle_domain_command(
    config: &Config,
    project: &str,
    subcommand: DomainSubcommand,
) -> Result<()> {
    let http_client = Client::new();
    let token = config.get_token().ok_or_else(|| {
        anyhow::anyhow!("Not authenticated. Please run 'rise login' first")
    })?;

    let backend_url = config.get_backend_url();
    
    match subcommand {
        DomainSubcommand::Add { domain } => {
            add_domain(
                &http_client,
                &backend_url,
                &token,
                project,
                &domain,
            )
            .await?;
        }
        DomainSubcommand::List => {
            list_domains(&http_client, &backend_url, &token, project).await?;
        }
        DomainSubcommand::Delete { domain } => {
            delete_domain(
                &http_client,
                &backend_url,
                &token,
                project,
                &domain,
            )
            .await?;
        }
        DomainSubcommand::Verify { domain } => {
            verify_domain(
                &http_client,
                &backend_url,
                &token,
                project,
                &domain,
            )
            .await?;
        }
    }

    Ok(())
}

/// Domain management subcommands
#[derive(Debug, Clone)]
pub enum DomainSubcommand {
    Add { domain: String },
    List,
    Delete { domain: String },
    Verify { domain: String },
}
