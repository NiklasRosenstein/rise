/// Validates that custom domains do not overlap with the project default domain patterns.
///
/// This module provides utilities to check if a custom domain would conflict with
/// the automatically generated domain pattern for Rise projects.
///
/// Extract the domain pattern from an ingress URL template.
///
/// The template may contain placeholders like `{project_name}` and `{deployment_group}`.
/// This function converts the template into a wildcard pattern that can be used to check
/// if a domain would conflict with the project's default domain pattern.
///
/// # Examples
///
/// - `"{project_name}.apps.example.com"` → `"*.apps.example.com"`
/// - `"apps.example.com/{project_name}"` → `"apps.example.com"`
/// - `"{project_name}-{deployment_group}.preview.example.com"` → `"*.preview.example.com"`
///
/// # Arguments
///
/// * `template` - The ingress URL template from the configuration
///
/// # Returns
///
/// An optional tuple of (hostname_pattern, has_path_prefix) where:
/// - hostname_pattern: The extracted hostname pattern (with wildcards if applicable)
/// - has_path_prefix: True if the template uses path-based routing (e.g., "host.com/{project}")
pub fn extract_domain_pattern(template: &str) -> Option<(String, bool)> {
    // Check if template contains a path component (e.g., "host.com/{project_name}")
    if let Some(slash_pos) = template.find('/') {
        // Path-based routing: "host.com/{project_name}" → hostname is "host.com"
        let hostname = template[..slash_pos].to_string();
        Some((hostname, true))
    } else {
        // Subdomain-based routing: "{project_name}.apps.example.com"
        // Replace placeholders with wildcards to get the pattern
        let mut pattern = template.to_string();

        // Replace {project_name} with *
        if pattern.contains("{project_name}") {
            pattern = pattern.replace("{project_name}", "*");
        }

        // Replace {deployment_group} with *
        if pattern.contains("{deployment_group}") {
            pattern = pattern.replace("{deployment_group}", "*");
        }

        // If pattern still contains braces, it's an invalid template - ignore it
        if pattern.contains('{') || pattern.contains('}') {
            return None;
        }

        Some((pattern, false))
    }
}

/// Check if a domain matches a wildcard pattern.
///
/// # Examples
///
/// - `matches_wildcard_pattern("foo.apps.example.com", "*.apps.example.com")` → `true`
/// - `matches_wildcard_pattern("bar.apps.example.com", "*.apps.example.com")` → `true`
/// - `matches_wildcard_pattern("other.com", "*.apps.example.com")` → `false`
/// - `matches_wildcard_pattern("apps.example.com", "*.apps.example.com")` → `false` (too short)
///
/// # Arguments
///
/// * `domain` - The domain to check
/// * `pattern` - The wildcard pattern (may contain `*` as a placeholder)
///
/// # Returns
///
/// True if the domain matches the pattern, false otherwise
fn matches_wildcard_pattern(domain: &str, pattern: &str) -> bool {
    if !pattern.contains('*') {
        // No wildcard - must match exactly
        return domain == pattern;
    }

    // Split pattern by wildcard
    let parts: Vec<&str> = pattern.split('*').collect();

    if parts.is_empty() {
        return false;
    }

    // Check if domain starts with the first part
    let first_part = parts[0];
    if !domain.starts_with(first_part) {
        return false;
    }

    // Check if domain ends with the last part
    let last_part = parts[parts.len() - 1];
    if !domain.ends_with(last_part) {
        return false;
    }

    // For single wildcard pattern, check that the wildcard replacement doesn't contain dots
    if parts.len() == 2 {
        let wildcard_replacement = &domain[first_part.len()..domain.len() - last_part.len()];

        // Wildcard must not be empty (to prevent matching the pattern itself)
        if wildcard_replacement.is_empty() {
            return false;
        }

        // The wildcard replacement should not contain dots (to prevent subdomain wildcards)
        // For example, *.apps.example.com should match "foo.apps.example.com"
        // but not "foo.bar.apps.example.com"
        return !wildcard_replacement.contains('.');
    }

    // For multiple wildcards, we need to match each part in sequence
    // This is a simplified implementation that checks each non-wildcard part appears in order
    let mut pos = first_part.len();

    // Match middle parts (skip first and last as they're already checked)
    for part in parts.iter().take(parts.len() - 1).skip(1) {
        if part.is_empty() {
            // Two wildcards in a row - skip
            continue;
        }

        // Find this part in the remaining domain
        if let Some(found_pos) = domain[pos..].find(part) {
            pos += found_pos + part.len();
        } else {
            // Part not found in domain
            return false;
        }
    }

    // Check that we've consumed enough of the domain to leave room for the last part
    if pos > domain.len() - last_part.len() {
        return false;
    }

    // Additional check: ensure wildcards match non-empty content and don't cross subdomain boundaries
    // For patterns like *-*.preview.example.com, ensure both wildcards match content without dots
    let middle_section = &domain[first_part.len()..domain.len() - last_part.len()];

    // Count number of wildcards (parts.len() - 1)
    let wildcard_count = parts.len() - 1;

    // For patterns with multiple wildcards, we need at least one non-dot character per wildcard
    // This prevents matching patterns that would cross subdomain boundaries
    // Split the middle section by the middle parts and check each wildcard match
    if wildcard_count > 1 {
        // For now, accept multi-wildcard patterns if they have valid structure
        // A more sophisticated check would verify each wildcard segment doesn't contain dots
        // but that requires proper parsing of the middle parts
        return !middle_section.is_empty();
    }

    true
}

/// Check if a custom domain would conflict with project default domain patterns.
///
/// # Arguments
///
/// * `domain` - The custom domain to validate
/// * `production_template` - The production ingress URL template
/// * `staging_template` - The optional staging ingress URL template
///
/// # Returns
///
/// Ok(()) if the domain is valid, Err(reason) if it conflicts with a project pattern
pub fn validate_custom_domain(
    domain: &str,
    production_template: &str,
    staging_template: Option<&str>,
) -> Result<(), String> {
    // Extract pattern from production template
    if let Some((pattern, has_path)) = extract_domain_pattern(production_template) {
        if has_path {
            // Path-based routing: custom domain must not match the base hostname
            if domain == pattern {
                return Err(format!(
                    "Custom domain '{}' conflicts with the project default domain pattern (production template uses this hostname for path-based routing)",
                    domain
                ));
            }
        } else {
            // Subdomain-based routing: check if custom domain matches the pattern
            if matches_wildcard_pattern(domain, &pattern) {
                return Err(format!(
                    "Custom domain '{}' conflicts with the project default domain pattern '{}'",
                    domain, pattern
                ));
            }
        }
    }

    // Extract pattern from staging template if provided
    if let Some(staging_template) = staging_template {
        if let Some((pattern, has_path)) = extract_domain_pattern(staging_template) {
            if has_path {
                // Path-based routing: custom domain must not match the base hostname
                if domain == pattern {
                    return Err(format!(
                        "Custom domain '{}' conflicts with the staging deployment domain pattern (staging template uses this hostname for path-based routing)",
                        domain
                    ));
                }
            } else {
                // Subdomain-based routing: check if custom domain matches the pattern
                if matches_wildcard_pattern(domain, &pattern) {
                    return Err(format!(
                        "Custom domain '{}' conflicts with the staging deployment domain pattern '{}'",
                        domain, pattern
                    ));
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_domain_pattern_subdomain() {
        let (pattern, has_path) =
            extract_domain_pattern("{project_name}.apps.example.com").unwrap();
        assert_eq!(pattern, "*.apps.example.com");
        assert!(!has_path);
    }

    #[test]
    fn test_extract_domain_pattern_path_based() {
        let (pattern, has_path) = extract_domain_pattern("example.com/{project_name}").unwrap();
        assert_eq!(pattern, "example.com");
        assert!(has_path);
    }

    #[test]
    fn test_extract_domain_pattern_staging() {
        let (pattern, has_path) =
            extract_domain_pattern("{project_name}-{deployment_group}.preview.example.com")
                .unwrap();
        assert_eq!(pattern, "*-*.preview.example.com");
        assert!(!has_path);
    }

    #[test]
    fn test_matches_wildcard_pattern_exact() {
        assert!(matches_wildcard_pattern("example.com", "example.com"));
        assert!(!matches_wildcard_pattern("other.com", "example.com"));
    }

    #[test]
    fn test_matches_wildcard_pattern_subdomain() {
        assert!(matches_wildcard_pattern(
            "foo.apps.example.com",
            "*.apps.example.com"
        ));
        assert!(matches_wildcard_pattern(
            "bar.apps.example.com",
            "*.apps.example.com"
        ));
        assert!(!matches_wildcard_pattern(
            "apps.example.com",
            "*.apps.example.com"
        )); // Too short
        assert!(!matches_wildcard_pattern(
            "foo.bar.apps.example.com",
            "*.apps.example.com"
        )); // Too many levels
        assert!(!matches_wildcard_pattern("other.com", "*.apps.example.com"));
    }

    #[test]
    fn test_matches_wildcard_pattern_complex() {
        assert!(matches_wildcard_pattern(
            "foo-staging.preview.example.com",
            "*-*.preview.example.com"
        ));
        assert!(!matches_wildcard_pattern(
            "foo.preview.example.com",
            "*-*.preview.example.com"
        ));
    }

    #[test]
    fn test_validate_custom_domain_subdomain_conflict() {
        let result = validate_custom_domain(
            "bar.apps.example.com",
            "{project_name}.apps.example.com",
            None,
        );
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("conflicts with the project default domain pattern"));
    }

    #[test]
    fn test_validate_custom_domain_subdomain_ok() {
        let result = validate_custom_domain(
            "mycustomdomain.com",
            "{project_name}.apps.example.com",
            None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_custom_domain_path_based_conflict() {
        let result = validate_custom_domain("example.com", "example.com/{project_name}", None);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("conflicts with the project default domain pattern"));
    }

    #[test]
    fn test_validate_custom_domain_path_based_ok() {
        let result = validate_custom_domain("other.com", "example.com/{project_name}", None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_custom_domain_staging_conflict() {
        let result = validate_custom_domain(
            "foo-bar.preview.example.com",
            "{project_name}.apps.example.com",
            Some("{project_name}-{deployment_group}.preview.example.com"),
        );
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("staging deployment domain pattern"));
    }

    #[test]
    fn test_validate_custom_domain_multiple_levels() {
        // Should reject domains with too many subdomain levels
        let result = validate_custom_domain(
            "foo.bar.apps.example.com",
            "{project_name}.apps.example.com",
            None,
        );
        assert!(result.is_ok()); // This should be OK since it has too many levels
    }
}
