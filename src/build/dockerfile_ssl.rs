// Dockerfile SSL certificate preprocessing
//
// Injects BuildKit secret mounts into RUN commands to make SSL certificates
// available during the build process without baking them into the image.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tracing::debug;

use super::ssl::{SSL_CERT_PATHS, SSL_ENV_VARS};

/// Name of the SSL certificate build context used in BuildKit
pub(crate) const SSL_CERT_BUILD_CONTEXT: &str = "rise-ssl-cert";

/// RAII struct for managing SSL certificate build context
///
/// When using bind mount strategy for large certificates, this creates a temporary
/// directory containing the certificate and automatically cleans it up when dropped.
pub(crate) struct SslCertContext {
    _temp_dir: TempDir,
    /// Path to the temporary directory containing ca-certificates.crt
    pub context_path: PathBuf,
}

impl SslCertContext {
    /// Create SSL cert build context for bind mount strategy
    ///
    /// Creates a temp directory with ca-certificates.crt inside, suitable for use as
    /// a named build context that keeps the cert separate from the main build context.
    pub fn new(ssl_cert_path: &Path) -> Result<Self> {
        let temp_dir =
            TempDir::new().context("Failed to create temp directory for SSL certificate")?;
        let cert_dest = temp_dir.path().join("ca-certificates.crt");
        std::fs::copy(ssl_cert_path, &cert_dest).with_context(|| {
            format!(
                "Failed to copy SSL certificate to temp directory: {}",
                cert_dest.display()
            )
        })?;

        debug!(
            "Created SSL cert build context in temp directory: {}",
            temp_dir.path().display()
        );

        Ok(Self {
            context_path: temp_dir.path().to_path_buf(),
            _temp_dir: temp_dir,
        })
    }
}

/// Generate the mount specification string for all SSL certificate paths
///
/// Always uses bind mount strategy with a named build context to avoid BuildKit's
/// 500KiB secret size limit and ensure the certificate cannot be copied into the
/// image via COPY commands.
fn generate_ssl_mount_spec() -> String {
    SSL_CERT_PATHS
        .iter()
        .map(|path| {
            format!(
                "--mount=type=bind,from={},source=ca-certificates.crt,target={},readonly",
                SSL_CERT_BUILD_CONTEXT, path
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Generate export statements for all SSL environment variables
fn generate_ssl_exports(ssl_cert_path: &str) -> String {
    SSL_ENV_VARS
        .iter()
        .map(|var| format!("export {}={}", var, ssl_cert_path))
        .collect::<Vec<_>>()
        .join(" && ")
}

/// Check if a line is a RUN instruction
fn is_run_instruction(line: &str) -> bool {
    let trimmed = line.trim();
    let upper = trimmed.to_uppercase();
    upper.starts_with("RUN ")
        || upper.starts_with("RUN\t")
        || upper == "RUN"
        || (upper.starts_with("RUN")
            && trimmed.chars().nth(3).is_some_and(|c| !c.is_alphanumeric()))
}

/// Inject SSL mount specification into a RUN line
fn inject_mount_into_run(line: &str, mount_spec: &str) -> String {
    let trimmed = line.trim_start();

    // Find the position of "RUN" (case-insensitive)
    let run_upper = trimmed.to_uppercase();
    let run_pos = run_upper.find("RUN").unwrap();
    let after_run = run_pos + 3;

    // Get leading whitespace from original line
    let leading_ws_len = line.len() - trimmed.len();
    let leading_ws = &line[..leading_ws_len];

    // Get the part after RUN
    let rest = &trimmed[after_run..];

    // Check if there's content after RUN
    if rest.is_empty() {
        // RUN on its own line (continuation expected)
        format!("{}{} {}", leading_ws, &trimmed[..after_run], mount_spec)
    } else if rest.starts_with(char::is_whitespace) {
        // Find the first non-whitespace character
        let ws_end = rest
            .find(|c: char| !c.is_whitespace())
            .unwrap_or(rest.len());
        let ws = &rest[..ws_end];
        let command = &rest[ws_end..];

        // Separate existing RUN flags from the actual command
        let (flags, actual_command) = extract_run_flags(command);

        // Use the first SSL_CERT_PATH as the environment variable
        let ssl_cert_path = SSL_CERT_PATHS[0];

        // Build the new command with all SSL env vars
        let wrapped_command = if actual_command.is_empty() {
            // No command yet (continuation expected)
            String::new()
        } else {
            format!(
                "{} && {}",
                generate_ssl_exports(ssl_cert_path),
                actual_command
            )
        };

        format!(
            "{}{}{}{} {}",
            leading_ws,
            &trimmed[..after_run],
            ws,
            mount_spec,
            if flags.is_empty() {
                wrapped_command
            } else {
                format!("{} {}", flags, wrapped_command)
            }
        )
    } else {
        // No whitespace after RUN (unusual but handle it)
        let (flags, actual_command) = extract_run_flags(rest);
        let ssl_cert_path = SSL_CERT_PATHS[0];

        let wrapped_command = if actual_command.is_empty() {
            String::new()
        } else {
            format!(
                "{} && {}",
                generate_ssl_exports(ssl_cert_path),
                actual_command
            )
        };

        format!(
            "{}{} {} {}",
            leading_ws,
            &trimmed[..after_run],
            mount_spec,
            if flags.is_empty() {
                wrapped_command
            } else {
                format!("{} {}", flags, wrapped_command)
            }
        )
    }
}

/// Extract RUN flags from a command string
/// Returns (flags, command) where flags are the --mount and other RUN options
fn extract_run_flags(command: &str) -> (String, String) {
    let mut flags = Vec::new();
    let mut parts = command.split_whitespace().peekable();
    let mut actual_command_parts = Vec::new();

    while let Some(part) = parts.peek() {
        if part.starts_with("--") {
            // This is a flag
            let flag = parts.next().unwrap();
            flags.push(flag.to_string());

            // Check if this flag takes a value (e.g., --mount=... is one token, but --mount ... might be two)
            if !flag.contains('=') && parts.peek().is_some() {
                let next = parts.peek().unwrap();
                if !next.starts_with("--") {
                    // This is the flag's value
                    flags.push(parts.next().unwrap().to_string());
                }
            }
        } else {
            // Rest is the actual command
            break;
        }
    }

    // Collect remaining parts as the actual command
    actual_command_parts.extend(parts);

    (flags.join(" "), actual_command_parts.join(" "))
}

/// Inject SSL certificate bind mounts into RUN commands in a Dockerfile
fn inject_ssl_mounts(dockerfile_content: &str) -> String {
    let mount_spec = generate_ssl_mount_spec();
    let mut result = String::new();
    let lines: Vec<&str> = dockerfile_content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        if is_run_instruction(line) {
            // Collect all lines of this RUN instruction
            let mut run_lines = vec![line];
            let mut j = i;
            while j < lines.len() && lines[j].trim_end().ends_with('\\') {
                j += 1;
                if j < lines.len() {
                    run_lines.push(lines[j]);
                }
            }

            // Process the complete RUN instruction
            let modified = inject_mount_into_multiline_run(&run_lines, &mount_spec);
            result.push_str(&modified);

            // Skip the lines we just processed
            i = j + 1;
        } else {
            result.push_str(line);
            result.push('\n');
            i += 1;
        }
    }

    result
}

/// Inject SSL mounts into a potentially multiline RUN instruction
fn inject_mount_into_multiline_run(run_lines: &[&str], mount_spec: &str) -> String {
    if run_lines.is_empty() {
        return String::new();
    }

    // If single line, use the existing function
    if run_lines.len() == 1 {
        return format!("{}\n", inject_mount_into_run(run_lines[0], mount_spec));
    }

    // Multiline RUN - need to extract ALL flags from ALL lines
    let first_line = run_lines[0];

    // Extract RUN prefix and whitespace
    let trimmed = first_line.trim_start();
    let leading_ws_len = first_line.len() - trimmed.len();
    let leading_ws = &first_line[..leading_ws_len];

    let run_upper = trimmed.to_uppercase();
    let run_pos = run_upper.find("RUN").unwrap();
    let after_run = run_pos + 3;

    // Collect all flags and the actual command from all lines
    let mut all_flags = Vec::new();
    let mut command_lines = Vec::new();
    let mut found_command = false;

    for (idx, &line) in run_lines.iter().enumerate() {
        let content = if idx == 0 {
            // First line: skip "RUN" and leading whitespace
            let rest = &trimmed[after_run..];
            rest.trim_start()
        } else {
            // Continuation line: just trim leading whitespace
            line.trim_start()
        };

        // Check if this line ends with backslash
        let has_continuation = content.trim_end().ends_with('\\');
        let content_no_backslash = if has_continuation {
            content.trim_end().strip_suffix('\\').unwrap().trim_end()
        } else {
            content
        };

        // Extract flags from this line
        let (flags, command) = extract_run_flags(content_no_backslash);

        if !flags.is_empty() {
            all_flags.push(flags);
        }

        if !command.is_empty() {
            found_command = true;
            if has_continuation {
                command_lines.push(format!("{} \\", command));
            } else {
                command_lines.push(command.to_string());
            }
        } else if found_command {
            // Empty command part but we've already found the command
            // This shouldn't happen in well-formed Dockerfiles
            if has_continuation {
                command_lines.push("\\".to_string());
            }
        }
    }

    let ssl_cert_path = SSL_CERT_PATHS[0];
    let all_flags_str = all_flags.join(" ");

    // Build the result
    let mut result = Vec::new();

    if command_lines.is_empty() {
        // No command found (shouldn't happen)
        result.push(format!("{}RUN {}", leading_ws, mount_spec));
    } else if command_lines.len() == 1 {
        // Single line command
        if all_flags_str.is_empty() {
            result.push(format!(
                "{}RUN {} {} && {}",
                leading_ws,
                mount_spec,
                generate_ssl_exports(ssl_cert_path),
                command_lines[0]
            ));
        } else {
            result.push(format!(
                "{}RUN {} {} {} && {}",
                leading_ws,
                mount_spec,
                all_flags_str,
                generate_ssl_exports(ssl_cert_path),
                command_lines[0]
            ));
        }
    } else {
        // Multiline command - put export on first line with backslash, command on continuation
        if all_flags_str.is_empty() {
            result.push(format!(
                "{}RUN {} {} && \\",
                leading_ws,
                mount_spec,
                generate_ssl_exports(ssl_cert_path)
            ));
        } else {
            result.push(format!(
                "{}RUN {} {} {} && \\",
                leading_ws,
                mount_spec,
                all_flags_str,
                generate_ssl_exports(ssl_cert_path)
            ));
        }

        // Add all command lines with their original indentation
        for cmd_line in &command_lines {
            // Get the indentation from the original continuation line
            let original_line_idx = result.len();
            let original_indent = if original_line_idx < run_lines.len() {
                run_lines[original_line_idx]
                    .chars()
                    .take_while(|c| c.is_whitespace())
                    .collect::<String>()
            } else {
                // Default indentation if we don't have an original to reference
                "    ".to_string()
            };
            result.push(format!("{}{}", original_indent, cmd_line));
        }
    }

    result.join("\n") + "\n"
}

/// Preprocess a Dockerfile to inject SSL certificate mounts into RUN commands
///
/// This function:
/// 1. Reads the original Dockerfile
/// 2. Injects `--mount=type=bind,from=rise-ssl-cert,source=ca-certificates.crt,target=<path>,readonly`
///    into each RUN command for all common SSL certificate paths
/// 3. Exports all SSL environment variables before the command:
///    - SSL_CERT_FILE (curl, wget, Git)
///    - NIX_SSL_CERT_FILE (Nix package manager)
///    - NODE_EXTRA_CA_CERTS (Node.js and npm)
///    - REQUESTS_CA_BUNDLE (Python requests library)
///    - AWS_CA_BUNDLE (AWS SDK/CLI)
/// 4. Writes the processed Dockerfile to a temporary directory
/// 5. Returns the temp directory (for lifetime) and the path to the processed file
///
/// Using `export` ensures the variables are available for all commands in the RUN instruction,
/// including multiline commands with backslash continuations.
///
/// The caller should:
/// 1. Create an SslCertContext to set up the named build context
/// 2. Pass `--build-context rise-ssl-cert=<context_path>` to buildx
/// 3. Or pass `--local rise-ssl-cert=<context_path>` to buildctl
///
/// Returns:
/// - TempDir: Temporary directory containing the processed Dockerfile (must be kept alive)
/// - PathBuf: Path to the processed Dockerfile
pub(crate) fn preprocess_dockerfile_for_ssl(
    original_dockerfile: &Path,
) -> Result<(TempDir, PathBuf)> {
    let content = std::fs::read_to_string(original_dockerfile).with_context(|| {
        format!(
            "Failed to read Dockerfile: {}",
            original_dockerfile.display()
        )
    })?;

    let processed = inject_ssl_mounts(&content);

    debug!("Processed Dockerfile with SSL mounts:\n{}", processed);

    // Write to temp directory, preserving the original filename
    let temp_dir = TempDir::new().context("Failed to create temp directory")?;
    let filename = original_dockerfile
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("Dockerfile"));
    let temp_dockerfile = temp_dir.path().join(filename);
    std::fs::write(&temp_dockerfile, processed).context("Failed to write processed Dockerfile")?;

    Ok((temp_dir, temp_dockerfile))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_run_instruction() {
        assert!(is_run_instruction("RUN apt-get update"));
        assert!(is_run_instruction("RUN\tapt-get update"));
        assert!(is_run_instruction("  RUN apt-get update"));
        assert!(is_run_instruction("RUN"));
        assert!(is_run_instruction("run apt-get update")); // case insensitive
        assert!(!is_run_instruction("RUNNER something"));
        assert!(!is_run_instruction("# RUN apt-get update"));
        assert!(!is_run_instruction("FROM ubuntu"));
    }

    #[test]
    fn test_inject_mount_into_run() {
        let mount_spec = generate_ssl_mount_spec();

        // Simple RUN command
        let result = inject_mount_into_run("RUN apt-get update", &mount_spec);
        assert!(result.contains(&mount_spec));
        // Verify all 5 SSL environment variables are exported
        assert!(result.contains("export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(result.contains("export NIX_SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(result.contains("export NODE_EXTRA_CA_CERTS=/etc/ssl/certs/ca-certificates.crt"));
        assert!(result.contains("export REQUESTS_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(result.contains("export AWS_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(result.contains("&& apt-get update"));
        assert!(!result.contains("("));

        // RUN with existing mount - we always inject (mounts and exports)
        // This ensures SSL env vars are always added, even if there are existing mounts
        let line_with_mount =
            "RUN --mount=type=bind,source=some-file,target=/app/file apt-get update";
        let result = inject_mount_into_run(line_with_mount, &mount_spec);
        assert!(result.contains(&mount_spec));
        assert!(result.contains("--mount=type=bind,source=some-file,target=/app/file"));
        assert!(result.contains("export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(result.contains("&& apt-get update"));

        // RUN with leading whitespace
        let result = inject_mount_into_run("    RUN apt-get update", &mount_spec);
        assert!(result.starts_with("    RUN"));
        assert!(result.contains(&mount_spec));
        // Verify all 5 SSL environment variables are exported
        assert!(result.contains("export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(result.contains("export NIX_SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(result.contains("export NODE_EXTRA_CA_CERTS=/etc/ssl/certs/ca-certificates.crt"));
        assert!(result.contains("export REQUESTS_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(result.contains("export AWS_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(result.contains("&& apt-get update"));

        // RUN with existing flags
        let result = inject_mount_into_run("RUN --network=host apt-get update", &mount_spec);
        assert!(result.contains(&mount_spec));
        assert!(result.contains("--network=host"));
        // Verify all 5 SSL environment variables are exported
        assert!(result.contains("export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(result.contains("export NIX_SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(result.contains("export NODE_EXTRA_CA_CERTS=/etc/ssl/certs/ca-certificates.crt"));
        assert!(result.contains("export REQUESTS_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(result.contains("export AWS_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(result.contains("&& apt-get update"));
    }

    #[test]
    fn test_extract_run_flags() {
        // No flags
        let (flags, command) = extract_run_flags("apt-get update");
        assert_eq!(flags, "");
        assert_eq!(command, "apt-get update");

        // One flag with =
        let (flags, command) = extract_run_flags("--network=host apt-get update");
        assert_eq!(flags, "--network=host");
        assert_eq!(command, "apt-get update");

        // Multiple flags
        let (flags, command) =
            extract_run_flags("--network=host --mount=type=cache,target=/cache apt-get update");
        assert_eq!(flags, "--network=host --mount=type=cache,target=/cache");
        assert_eq!(command, "apt-get update");

        // Command with multiple words
        let (flags, command) =
            extract_run_flags("--network=host apt-get update && apt-get install curl");
        assert_eq!(flags, "--network=host");
        assert_eq!(command, "apt-get update && apt-get install curl");
    }

    #[test]
    fn test_inject_ssl_mounts() {
        let dockerfile = r#"FROM ubuntu:22.04
RUN apt-get update && apt-get install -y curl
COPY . /app
RUN pip install -r requirements.txt
CMD ["python", "app.py"]
"#;

        let result = inject_ssl_mounts(dockerfile);

        // Should contain bind mount spec in RUN lines
        assert!(result.contains("--mount=type=bind,from=rise-ssl-cert"));

        // Should contain all SSL environment variables
        assert!(result.contains("export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(result.contains("export NIX_SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(result.contains("export NODE_EXTRA_CA_CERTS=/etc/ssl/certs/ca-certificates.crt"));
        assert!(result.contains("export REQUESTS_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(result.contains("export AWS_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt"));

        // Should not wrap commands in parentheses
        assert!(!result.contains("("));
        assert!(result.contains("&& apt-get update && apt-get install -y curl"));
        assert!(result.contains("&& pip install -r requirements.txt"));

        // Should preserve FROM and COPY
        assert!(result.contains("FROM ubuntu:22.04"));
        assert!(result.contains("COPY . /app"));

        // Count the number of RUN lines with mounts (should be 2)
        let mount_count = result
            .lines()
            .filter(|line| {
                line.contains("RUN") && line.contains("--mount=type=bind,from=rise-ssl-cert")
            })
            .count();
        assert_eq!(mount_count, 2);
    }

    #[test]
    fn test_multiline_run() {
        // Test the exact case from the error message
        let dockerfile = r#"FROM ubuntu:22.04
RUN apt-get update -y && \
    apt-get install -y gzip zip jq git less && \
    apt-get clean
"#;

        let result = inject_ssl_mounts(dockerfile);
        println!("Result:\n{}", result);

        let lines: Vec<&str> = result.lines().collect();

        // First line should have mount, all SSL exports, and the backslash at the end
        assert!(lines[1].contains("--mount=type=bind,from=rise-ssl-cert"));
        assert!(lines[1].contains("export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(lines[1].contains("export NIX_SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(lines[1].contains("export NODE_EXTRA_CA_CERTS=/etc/ssl/certs/ca-certificates.crt"));
        assert!(lines[1].contains("export REQUESTS_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(lines[1].contains("export AWS_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(
            lines[1].ends_with(" \\"),
            "First line should end with backslash continuation"
        );

        // Continuation lines should contain the actual commands
        assert!(lines[2].trim().starts_with("apt-get update"));
        assert!(lines[3].trim().starts_with("apt-get install"));
        assert!(lines[4].trim().starts_with("apt-get clean"));

        // Verify no mount or export on continuation lines
        assert!(!lines[2].contains("--mount=type=bind,from=rise-ssl-cert"));
        assert!(!lines[2].contains("export"));

        // Verify no parentheses anywhere
        assert!(!result.contains("("));
        assert!(!result.contains(")"));
    }

    #[test]
    fn test_run_with_existing_mount_flag() {
        // Test RUN command with existing --mount flag (like uv.lock binding)
        let dockerfile = r#"FROM python:3.12
RUN --mount=type=bind,source=uv.lock,target=uv.lock uv sync --locked
"#;

        let result = inject_ssl_mounts(dockerfile);

        // Should have both SSL bind mounts and the original bind mount
        assert!(result.contains("--mount=type=bind,from=rise-ssl-cert"));
        assert!(result.contains("--mount=type=bind,source=uv.lock,target=uv.lock"));

        // All SSL environment variables should be exported
        assert!(result.contains("export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(result.contains("export NIX_SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(result.contains("export NODE_EXTRA_CA_CERTS=/etc/ssl/certs/ca-certificates.crt"));
        assert!(result.contains("export REQUESTS_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(result.contains("export AWS_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt"));

        // The shell command should be "uv sync --locked"
        assert!(result.contains("&& uv sync --locked"));

        // Verify order: RUN, then all --mount flags, then export, then actual command
        let run_line = result.lines().nth(1).unwrap();

        // All --mount flags should come before "export"
        let export_pos = run_line.find("export").expect("Should contain export");
        let bind_mount_pos = run_line
            .find("--mount=type=bind")
            .expect("Should contain bind mount");
        assert!(
            bind_mount_pos < export_pos,
            "Bind mount should come before export. Line: {}",
            run_line
        );

        // The actual command should come after export
        let command_pos = run_line.find("uv sync").expect("Should contain uv sync");
        assert!(
            export_pos < command_pos,
            "export should come before the command"
        );

        println!("Generated line: {}", run_line);
    }

    #[test]
    fn test_run_with_multiple_mount_flags_across_lines() {
        // Test the real-world case from the user's Containerfile
        let dockerfile = r#"FROM python:3.12
RUN --mount=type=bind,source=pyproject.toml,target=pyproject.toml \
    --mount=type=bind,source=uv.lock,target=uv.lock \
    uv sync --locked
"#;

        let result = inject_ssl_mounts(dockerfile);
        println!("Result:\n{}", result);

        let lines: Vec<&str> = result.lines().collect();
        let run_line = lines[1];

        // Should have SSL bind mounts and both original bind mounts
        assert!(run_line.contains("--mount=type=bind,from=rise-ssl-cert"));
        assert!(run_line.contains("--mount=type=bind,source=pyproject.toml,target=pyproject.toml"));
        assert!(run_line.contains("--mount=type=bind,source=uv.lock,target=uv.lock"));

        // All SSL environment variables should be exported
        assert!(run_line.contains("export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(run_line.contains("export NIX_SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(run_line.contains("export NODE_EXTRA_CA_CERTS=/etc/ssl/certs/ca-certificates.crt"));
        assert!(run_line.contains("export REQUESTS_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(run_line.contains("export AWS_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt"));

        // The command should be present
        assert!(run_line.contains("uv sync --locked"));

        // Verify order: RUN, then all --mount flags, then export, then command
        let export_pos = run_line.find("export").expect("Should contain export");
        let pyproject_mount_pos = run_line
            .find("--mount=type=bind,source=pyproject.toml")
            .expect("Should contain pyproject mount");
        let uvlock_mount_pos = run_line
            .find("--mount=type=bind,source=uv.lock")
            .expect("Should contain uv.lock mount");
        let command_pos = run_line.find("uv sync").expect("Should contain uv sync");

        // Both mounts should come before export
        assert!(
            pyproject_mount_pos < export_pos,
            "pyproject mount should come before export"
        );
        assert!(
            uvlock_mount_pos < export_pos,
            "uv.lock mount should come before export"
        );
        // Export should come before command
        assert!(
            export_pos < command_pos,
            "export should come before command"
        );

        println!("Generated line: {}", run_line);
    }
}
