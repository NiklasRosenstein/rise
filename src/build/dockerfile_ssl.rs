// Dockerfile SSL certificate preprocessing
//
// Injects BuildKit secret mounts into RUN commands to make SSL certificates
// available during the build process without baking them into the image.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tracing::debug;

use super::ssl::SSL_CERT_PATHS;

/// Generate the mount specification string for all SSL certificate paths
fn generate_ssl_mount_spec() -> String {
    SSL_CERT_PATHS
        .iter()
        .map(|path| format!("--mount=type=secret,id=SSL_CERT_FILE,target={}", path))
        .collect::<Vec<_>>()
        .join(" ")
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

        // Already has mount specification for SSL_CERT_FILE?
        if command.contains("--mount=type=secret,id=SSL_CERT_FILE") {
            return line.to_string();
        }

        // Separate existing RUN flags from the actual command
        let (flags, actual_command) = extract_run_flags(command);

        // Use the first SSL_CERT_PATH as the environment variable
        let ssl_cert_path = SSL_CERT_PATHS[0];

        // Build the new command with SSL_CERT_FILE env var
        let wrapped_command = if actual_command.is_empty() {
            // No command yet (continuation expected)
            String::new()
        } else {
            format!("SSL_CERT_FILE={} && ( {} )", ssl_cert_path, actual_command)
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
            format!("SSL_CERT_FILE={} && ( {} )", ssl_cert_path, actual_command)
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

/// Inject SSL certificate secret mounts into RUN commands in a Dockerfile
fn inject_ssl_mounts(dockerfile_content: &str) -> String {
    let mount_spec = generate_ssl_mount_spec();
    let mut result = String::new();
    let mut in_continuation = false;

    for line in dockerfile_content.lines() {
        if in_continuation {
            // Inside a multi-line command, don't modify
            result.push_str(line);
            in_continuation = line.trim_end().ends_with('\\');
        } else if is_run_instruction(line) {
            // Inject mount spec into RUN command
            let modified = inject_mount_into_run(line, &mount_spec);
            result.push_str(&modified);
            in_continuation = line.trim_end().ends_with('\\');
        } else {
            result.push_str(line);
        }
        result.push('\n');
    }

    result
}

/// Preprocess a Dockerfile to inject SSL certificate mounts into RUN commands
///
/// When SSL_CERT_FILE is set, this function:
/// 1. Reads the original Dockerfile
/// 2. Injects `--mount=type=secret,id=SSL_CERT_FILE,target=<path>` into each RUN command
///    for all common SSL certificate paths
/// 3. Sets SSL_CERT_FILE environment variable and wraps the command in parentheses:
///    `RUN --mount=... SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt && ( original_command )`
/// 4. Writes the processed Dockerfile to a temporary directory
/// 5. Returns the temp directory (for lifetime) and the path to the processed file
///
/// The caller should pass `--secret id=SSL_CERT_FILE,src=<path>` to the build command.
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
        let mount_spec =
            "--mount=type=secret,id=SSL_CERT_FILE,target=/etc/ssl/certs/ca-certificates.crt";

        // Simple RUN command
        let result = inject_mount_into_run("RUN apt-get update", mount_spec);
        assert!(result.contains(mount_spec));
        assert!(result.contains("SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(result.contains("( apt-get update )"));
        assert!(result.contains("&&"));

        // RUN with existing mount (should not duplicate)
        let line_with_mount =
            "RUN --mount=type=secret,id=SSL_CERT_FILE,target=/etc/ssl apt-get update";
        let result = inject_mount_into_run(line_with_mount, mount_spec);
        assert_eq!(result, line_with_mount);

        // RUN with leading whitespace
        let result = inject_mount_into_run("    RUN apt-get update", mount_spec);
        assert!(result.starts_with("    RUN"));
        assert!(result.contains(mount_spec));
        assert!(result.contains("SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(result.contains("( apt-get update )"));

        // RUN with existing flags
        let result = inject_mount_into_run("RUN --network=host apt-get update", mount_spec);
        assert!(result.contains(mount_spec));
        assert!(result.contains("--network=host"));
        assert!(result.contains("SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(result.contains("( apt-get update )"));
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

        // Should contain mount spec in RUN lines
        assert!(result.contains("--mount=type=secret,id=SSL_CERT_FILE"));

        // Should contain SSL_CERT_FILE env var
        assert!(result.contains("SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt"));

        // Should wrap commands in parentheses
        assert!(result.contains("( apt-get update && apt-get install -y curl )"));
        assert!(result.contains("( pip install -r requirements.txt )"));

        // Should preserve FROM and COPY
        assert!(result.contains("FROM ubuntu:22.04"));
        assert!(result.contains("COPY . /app"));

        // Count the number of RUN lines with mounts (should be 2)
        let mount_count = result
            .lines()
            .filter(|line| {
                line.contains("RUN") && line.contains("--mount=type=secret,id=SSL_CERT_FILE")
            })
            .count();
        assert_eq!(mount_count, 2);
    }

    #[test]
    fn test_multiline_run() {
        let dockerfile = r#"FROM ubuntu:22.04
RUN apt-get update && \
    apt-get install -y curl && \
    rm -rf /var/lib/apt/lists/*
"#;

        let result = inject_ssl_mounts(dockerfile);

        // Mount and env var should only be on the first line
        let lines: Vec<&str> = result.lines().collect();
        assert!(lines[1].contains("--mount=type=secret,id=SSL_CERT_FILE"));
        assert!(lines[1].contains("SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt"));
        assert!(lines[1].contains("( apt-get update &&"));
        assert!(!lines[2].contains("--mount=type=secret,id=SSL_CERT_FILE"));
        assert!(!lines[2].contains("SSL_CERT_FILE="));
        assert!(!lines[3].contains("--mount=type=secret,id=SSL_CERT_FILE"));
        assert!(!lines[3].contains("SSL_CERT_FILE="));
    }
}
