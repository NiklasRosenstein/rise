// Railpack builds (buildx & buildctl variants)

use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use tracing::{debug, info, warn};

use super::buildkit::ensure_buildx_builder;
use super::proxy;
use super::registry::docker_push;
use super::ssl::embed_ssl_cert_in_plan;

/// BuildKit frontend type for buildctl
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum BuildctlFrontend {
    /// Standard Dockerfile frontend (dockerfile.v0)
    Dockerfile,
    /// Railpack gateway frontend (gateway.v0 + railpack-frontend)
    Railpack,
}

/// Options for building with Railpacks
pub(crate) struct RailpackBuildOptions<'a> {
    pub app_path: &'a str,
    pub image_tag: &'a str,
    pub container_cli: &'a str,
    pub buildx_supports_push: bool,
    pub use_buildctl: bool,
    pub push: bool,
    pub buildkit_host: Option<&'a str>,
    pub embed_ssl_cert: bool,
    pub env: &'a [String],
    pub no_cache: bool,
}

/// RAII guard for cleaning up temp files and directories
struct CleanupGuard {
    path: std::path::PathBuf,
    is_directory: bool,
}

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        if self.path.exists() {
            if self.is_directory {
                let _ = std::fs::remove_dir_all(&self.path);
                debug!("Cleaned up temp directory: {}", self.path.display());
            } else {
                let _ = std::fs::remove_file(&self.path);
                debug!("Cleaned up temp file: {}", self.path.display());
            }
        }
    }
}

/// Build image with Railpacks
pub(crate) fn build_image_with_railpacks(options: RailpackBuildOptions) -> Result<()> {
    // Check railpack CLI availability
    let railpack_check = Command::new("railpack").arg("--version").output();
    if railpack_check.is_err() {
        bail!(
            "railpack CLI not found. Ensure the railpack CLI is installed and available in PATH.\n\
             In production, this should be available in the rise-builder image."
        );
    }

    // Create .railpack-build directory in app_path
    let build_dir = Path::new(options.app_path).join(".railpack-build");
    let dir_existed = build_dir.exists();

    if !dir_existed {
        fs::create_dir(&build_dir).with_context(|| {
            format!("Failed to create build directory: {}", build_dir.display())
        })?;
    }

    let plan_file = build_dir.join("plan.json");
    let info_file = build_dir.join("info.json");

    // Set up cleanup guards
    // If we created the directory, clean up the entire directory
    // Otherwise, just clean up the individual files
    let _cleanup_guard = if !dir_existed {
        CleanupGuard {
            path: build_dir,
            is_directory: true,
        }
    } else {
        // When directory existed, we'll clean up files individually
        // Store the first file in the guard, we'll use a separate guard for the second
        CleanupGuard {
            path: plan_file.clone(),
            is_directory: false,
        }
    };

    let _info_guard = if dir_existed {
        Some(CleanupGuard {
            path: info_file.clone(),
            is_directory: false,
        })
    } else {
        None
    };

    // Read proxy vars and parse user-provided env vars before railpack prepare
    let proxy_vars = proxy::read_and_transform_proxy_vars();
    let user_env_vars = proxy::parse_env_vars(options.env)?;

    // Combine proxy vars and user env vars for secrets
    let mut all_secrets = proxy_vars.clone();
    all_secrets.extend(user_env_vars);

    info!("Running railpack prepare for: {}", options.app_path);

    // Run railpack prepare with --env flags so secrets are declared in the plan
    // (this enables railpack's secrets-hash cache invalidation mechanism)
    let mut cmd = Command::new("railpack");
    cmd.arg("prepare")
        .arg(options.app_path)
        .arg("--plan-out")
        .arg(&plan_file)
        .arg("--info-out")
        .arg(&info_file);

    for key in all_secrets.keys() {
        cmd.arg("--env")
            .arg(format!("{}={}", key, all_secrets[key]));
    }

    debug!("Executing command: {:?}", cmd);

    let status = cmd.status().context("Failed to execute railpack prepare")?;

    if !status.success() {
        bail!("railpack prepare failed with status: {}", status);
    }

    // Verify plan file was created
    if !plan_file.exists() {
        bail!(
            "railpack prepare did not create plan file at {}",
            plan_file.display()
        );
    }

    info!("✓ Railpack prepare completed");

    // Embed SSL certificate if requested
    if options.embed_ssl_cert {
        if let Some(ssl_cert_file) = super::env_var_non_empty("SSL_CERT_FILE") {
            let cert_path = Path::new(&ssl_cert_file);
            if cert_path.exists() {
                embed_ssl_cert_in_plan(&plan_file, cert_path)?;
            } else {
                warn!(
                    "SSL_CERT_FILE set to '{}' but file not found",
                    ssl_cert_file
                );
            }
        } else {
            warn!(
                "--railpack-embed-ssl-cert enabled but SSL_CERT_FILE environment variable not set"
            );
        }
    }

    // Debug log plan contents
    if let Ok(plan_contents) = fs::read_to_string(&plan_file) {
        debug!("Railpack plan.json contents:\n{}", plan_contents);
    }

    // Build with buildx or buildctl
    if options.use_buildctl {
        build_with_buildctl(
            options.app_path,
            &plan_file,
            options.image_tag,
            options.push,
            options.buildkit_host,
            &all_secrets,
            &HashMap::new(), // No local contexts for Railpack
            BuildctlFrontend::Railpack,
            options.no_cache,
        )?;
    } else {
        build_with_buildx(
            options.app_path,
            &plan_file,
            options.image_tag,
            options.container_cli,
            options.buildx_supports_push,
            options.push,
            options.buildkit_host,
            &all_secrets,
            options.no_cache,
        )?;
    }

    Ok(())
}

/// Build with docker buildx
#[allow(clippy::too_many_arguments)]
fn build_with_buildx(
    app_path: &str,
    plan_file: &Path,
    image_tag: &str,
    container_cli: &str,
    buildx_supports_push: bool,
    push: bool,
    buildkit_host: Option<&str>,
    secrets: &HashMap<String, String>,
    no_cache: bool,
) -> Result<()> {
    // Check buildx availability
    if !super::docker::is_buildx_available(container_cli) {
        bail!(
            "{} buildx not available. Install buildx or use railpack:buildctl backend instead.",
            container_cli
        );
    }

    info!(
        "Building image with {} buildx: {}",
        container_cli, image_tag
    );

    // If buildkit_host is provided, we need to create/use a builder pointing to it
    let builder_name = if let Some(host) = buildkit_host {
        Some(ensure_buildx_builder(container_cli, host)?)
    } else {
        None
    };

    let mut cmd = Command::new(container_cli);
    cmd.arg("buildx")
        .arg("build")
        .arg("--build-arg")
        .arg("BUILDKIT_SYNTAX=ghcr.io/railwayapp/railpack-frontend")
        .arg("-f")
        .arg(plan_file)
        .arg("-t")
        .arg(image_tag)
        .arg("--platform")
        .arg("linux/amd64");

    // Use the managed builder if available
    if let Some(ref builder) = builder_name {
        cmd.arg("--builder").arg(builder);
    }

    // Add no-cache flag if requested
    if no_cache {
        cmd.arg("--no-cache");
    }

    if push && buildx_supports_push {
        cmd.arg("--push");
    } else {
        // For local builds, use --load to ensure image is available in local daemon
        cmd.arg("--load");
    }

    // Add secrets
    for key in secrets.keys() {
        cmd.arg("--secret").arg(format!("id={},env={}", key, key));
    }

    // Add --add-host when a proxy URL was transformed to host.docker.internal.
    // Build containers inside BuildKit need this to resolve the host address.
    if proxy::needs_host_gateway(secrets) {
        if builder_name.is_some() {
            // Remote builders can't resolve the "host-gateway" magic value.
            // Resolve the actual gateway IP from the daemon container instead.
            if let Some(ref builder) = builder_name {
                if let Some(ip) =
                    super::buildkit::resolve_host_gateway_ip(container_cli, builder)
                {
                    cmd.arg("--add-host")
                        .arg(format!("host.docker.internal:{}", ip));
                }
            }
        } else {
            cmd.arg("--add-host")
                .arg("host.docker.internal:host-gateway");
        }
    }

    cmd.arg(app_path);

    debug!("Executing command: {:?}", cmd);

    let status = cmd
        .status()
        .with_context(|| format!("Failed to execute {} buildx build", container_cli))?;

    if !status.success() {
        bail!(
            "{} buildx build failed with status: {}",
            container_cli,
            status
        );
    }

    if push && !buildx_supports_push {
        docker_push(container_cli, image_tag)?;
    }

    Ok(())
}

/// Build with buildctl
///
/// Supports both Dockerfile and Railpack frontends:
/// - Dockerfile: Uses `--frontend=dockerfile.v0` for standard Dockerfiles
/// - Railpack: Uses `--frontend=gateway.v0` with railpack-frontend
///
/// The `secrets` HashMap contains environment variable secrets:
/// - key: environment variable name
/// - value: value is ignored (secrets are read from the current environment)
///
/// The `local_contexts` HashMap contains named build contexts:
/// - key: context name (e.g., "rise-internal-ssl-cert")
/// - value: local path to the context directory
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_with_buildctl(
    app_path: &str,
    dockerfile_or_plan: &Path,
    image_tag: &str,
    push: bool,
    buildkit_host: Option<&str>,
    secrets: &HashMap<String, String>,
    local_contexts: &HashMap<String, String>,
    frontend: BuildctlFrontend,
    no_cache: bool,
) -> Result<()> {
    // Check buildctl availability
    let buildctl_check = Command::new("buildctl").arg("--version").output();
    if buildctl_check.is_err() {
        bail!("buildctl not found. Install buildctl or use docker:buildx backend instead.");
    }

    info!("Building image with buildctl: {}", image_tag);

    let mut cmd = Command::new("buildctl");
    cmd.arg("build")
        .arg("--local")
        .arg(format!("context={}", app_path))
        .arg("--local")
        .arg(format!(
            "dockerfile={}",
            dockerfile_or_plan
                .parent()
                .unwrap_or(Path::new(app_path))
                .display()
        ));

    // Set frontend based on type
    match frontend {
        BuildctlFrontend::Dockerfile => {
            cmd.arg("--frontend=dockerfile.v0");
            // Add opt for filename if not the default "Dockerfile"
            if let Some(filename) = dockerfile_or_plan.file_name() {
                let filename_str = filename.to_string_lossy();
                if filename_str != "Dockerfile" {
                    cmd.arg("--opt").arg(format!("filename={}", filename_str));
                }
            }
        }
        BuildctlFrontend::Railpack => {
            cmd.arg("--frontend=gateway.v0")
                .arg("--opt")
                .arg("source=ghcr.io/railwayapp/railpack-frontend");
        }
    }

    cmd.arg("--output");

    // Set BUILDKIT_HOST if provided
    if let Some(host) = buildkit_host {
        cmd.env("BUILDKIT_HOST", host);
    }

    // Add local contexts (named build contexts)
    for (name, path) in local_contexts {
        cmd.arg("--local").arg(format!("{}={}", name, path));
    }

    // Add secrets
    for key in secrets.keys() {
        cmd.arg("--secret").arg(format!("id={},env={}", key, key));
    }

    // Add no-cache flag if requested
    if no_cache {
        cmd.arg("--no-cache");
    }

    if push {
        cmd.arg(format!(
            "type=image,name={},push=true,platform=linux/amd64",
            image_tag
        ));
    } else {
        cmd.arg(format!(
            "type=image,name={},platform=linux/amd64",
            image_tag
        ));
    }

    debug!("Executing command: {:?}", cmd);

    let status = cmd.status().context("Failed to execute buildctl build")?;

    if !status.success() {
        bail!("buildctl build failed with status: {}", status);
    }

    Ok(())
}
