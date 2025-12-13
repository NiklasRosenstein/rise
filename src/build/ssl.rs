// SSL certificate handling for railpack builds

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;
use tracing::{debug, info};

pub(crate) const SSL_CERT_PATHS: &[&str] = &[
    "/etc/ssl/certs/ca-certificates.crt", // Debian, Ubuntu, Arch, Gentoo, Slackware
    "/etc/pki/tls/certs/ca-bundle.crt",   // RedHat, CentOS, Fedora
    "/etc/ssl/ca-bundle.pem",             // OpenSUSE, SLES
    "/etc/ssl/cert.pem",                  // Alpine Linux
    "/usr/lib/ssl/cert.pem",              // OpenSSL (Default)
];

pub(crate) const SSL_ENV_VARS: &[&str] = &[
    "SSL_CERT_FILE",
    "NIX_SSL_CERT_FILE",
    "NODE_EXTRA_CA_CERTS",
    "REQUESTS_CA_BUNDLE",
    "AWS_CA_BUNDLE",
];

/// Embed SSL certificate into railpack plan.json
pub(crate) fn embed_ssl_cert_in_plan(plan_file: &Path, ssl_cert_file: &Path) -> Result<()> {
    use serde_json::Value;

    debug!(
        "Embedding SSL certificate from {} into {}",
        ssl_cert_file.display(),
        plan_file.display()
    );

    // Read and parse plan.json
    let plan_contents = fs::read_to_string(plan_file)
        .with_context(|| format!("Failed to read plan file: {}", plan_file.display()))?;

    let mut plan: Value = serde_json::from_str(&plan_contents)
        .with_context(|| format!("Failed to parse plan.json: {}", plan_file.display()))?;

    // Read SSL certificate contents
    let cert_contents = fs::read_to_string(ssl_cert_file).with_context(|| {
        format!(
            "Failed to read SSL certificate file: {}",
            ssl_cert_file.display()
        )
    })?;

    // Get the steps array
    let steps = plan
        .get_mut("steps")
        .and_then(|s| s.as_array_mut())
        .context("plan.json missing 'steps' array")?;

    if steps.is_empty() {
        bail!("plan.json has empty 'steps' array");
    }

    // Get the first step
    let first_step = &mut steps[0];

    // Add or update assets in the first step
    if !first_step.is_object() {
        bail!("First step in plan.json is not an object");
    }

    let first_step_obj = first_step.as_object_mut().unwrap();

    // Get or create assets object
    let assets = if let Some(assets) = first_step_obj.get_mut("assets") {
        assets
            .as_object_mut()
            .context("'assets' field is not an object")?
    } else {
        first_step_obj.insert("assets".to_string(), Value::Object(serde_json::Map::new()));
        first_step_obj
            .get_mut("assets")
            .unwrap()
            .as_object_mut()
            .unwrap()
    };

    // Add certificate to assets
    assets.insert("ssl_ca_cert".to_string(), Value::String(cert_contents));

    // Get or create commands array
    let commands = if let Some(commands) = first_step_obj.get_mut("commands") {
        commands
            .as_array_mut()
            .context("'commands' field is not an array")?
    } else {
        first_step_obj.insert("commands".to_string(), Value::Array(vec![]));
        first_step_obj
            .get_mut("commands")
            .unwrap()
            .as_array_mut()
            .unwrap()
    };

    // Create certificate installation command
    let cert_command = serde_json::json!({
        "name": "ssl_ca_cert",
        "path": "/etc/ssl/certs/ca-certificates.crt",
        "customName": "rise: install SSL certificate"
    });

    // Insert at the beginning of commands array
    commands.insert(0, cert_command);

    // Write modified plan back
    let modified_plan =
        serde_json::to_string_pretty(&plan).context("Failed to serialize modified plan.json")?;

    fs::write(plan_file, modified_plan).with_context(|| {
        format!(
            "Failed to write modified plan.json: {}",
            plan_file.display()
        )
    })?;

    info!("✓ Embedded SSL certificate into railpack plan");

    Ok(())
}
