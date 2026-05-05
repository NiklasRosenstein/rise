//! Kubernetes resource quantity parsing and validation utilities.
//!
//! Supports parsing CPU quantities (millicores) and memory quantities (bytes)
//! in standard Kubernetes formats.

use anyhow::{bail, Context, Result};

/// Parse a CPU quantity string into millicores.
///
/// Supported formats:
/// - `"500m"` → 500 (millicores)
/// - `"1"` → 1000 (1 core = 1000 millicores)
/// - `"2.5"` → 2500
pub fn parse_cpu_millicores(s: &str) -> Result<u64> {
    let s = s.trim();
    if s.is_empty() {
        bail!("empty CPU quantity");
    }

    if let Some(millis) = s.strip_suffix('m') {
        let value: u64 = millis
            .parse()
            .with_context(|| format!("invalid CPU millicores value: {s}"))?;
        Ok(value)
    } else {
        let value: f64 = s
            .parse()
            .with_context(|| format!("invalid CPU quantity: {s}"))?;
        if value < 0.0 {
            bail!("CPU quantity must be non-negative: {s}");
        }
        Ok((value * 1000.0).round() as u64)
    }
}

/// Parse a memory quantity string into bytes.
///
/// Supported formats:
/// - `"256Mi"` → 268435456
/// - `"1Gi"` → 1073741824
/// - `"512Ki"` → 524288
/// - `"1Ti"` → 1099511627776
/// - `"1048576"` → 1048576 (bare bytes)
pub fn parse_memory_bytes(s: &str) -> Result<u64> {
    let s = s.trim();
    if s.is_empty() {
        bail!("empty memory quantity");
    }

    let (num_str, multiplier) = if let Some(n) = s.strip_suffix("Ti") {
        (n, 1u64 << 40)
    } else if let Some(n) = s.strip_suffix("Gi") {
        (n, 1u64 << 30)
    } else if let Some(n) = s.strip_suffix("Mi") {
        (n, 1u64 << 20)
    } else if let Some(n) = s.strip_suffix("Ki") {
        (n, 1u64 << 10)
    } else {
        (s, 1)
    };

    let value: u64 = num_str
        .parse()
        .with_context(|| format!("invalid memory quantity: {s}"))?;
    Ok(value * multiplier)
}

/// Validate that a CPU value (as a string) falls within [min, max].
pub fn validate_cpu_range(value: &str, min: &str, max: &str) -> Result<()> {
    let v = parse_cpu_millicores(value).with_context(|| format!("invalid CPU value: {value}"))?;
    let lo = parse_cpu_millicores(min).with_context(|| format!("invalid min CPU: {min}"))?;
    let hi = parse_cpu_millicores(max).with_context(|| format!("invalid max CPU: {max}"))?;
    if v < lo || v > hi {
        bail!("CPU value {value} is outside the allowed range [{min}, {max}]");
    }
    Ok(())
}

/// Validate that a memory value (as a string) falls within [min, max].
pub fn validate_memory_range(value: &str, min: &str, max: &str) -> Result<()> {
    let v = parse_memory_bytes(value).with_context(|| format!("invalid memory value: {value}"))?;
    let lo = parse_memory_bytes(min).with_context(|| format!("invalid min memory: {min}"))?;
    let hi = parse_memory_bytes(max).with_context(|| format!("invalid max memory: {max}"))?;
    if v < lo || v > hi {
        bail!("Memory value {value} is outside the allowed range [{min}, {max}]");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cpu_millicores() {
        assert_eq!(parse_cpu_millicores("500m").unwrap(), 500);
        assert_eq!(parse_cpu_millicores("1").unwrap(), 1000);
        assert_eq!(parse_cpu_millicores("2").unwrap(), 2000);
        assert_eq!(parse_cpu_millicores("2.5").unwrap(), 2500);
        assert_eq!(parse_cpu_millicores("0.1").unwrap(), 100);
        assert_eq!(parse_cpu_millicores("100m").unwrap(), 100);
        assert_eq!(parse_cpu_millicores("1000m").unwrap(), 1000);
    }

    #[test]
    fn test_parse_cpu_millicores_errors() {
        assert!(parse_cpu_millicores("").is_err());
        assert!(parse_cpu_millicores("abc").is_err());
        assert!(parse_cpu_millicores("m").is_err());
    }

    #[test]
    fn test_parse_memory_bytes() {
        assert_eq!(parse_memory_bytes("256Mi").unwrap(), 256 * 1024 * 1024);
        assert_eq!(parse_memory_bytes("1Gi").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_memory_bytes("512Ki").unwrap(), 512 * 1024);
        assert_eq!(parse_memory_bytes("1Ti").unwrap(), 1u64 << 40);
        assert_eq!(parse_memory_bytes("1048576").unwrap(), 1048576);
        assert_eq!(parse_memory_bytes("64Mi").unwrap(), 64 * 1024 * 1024);
    }

    #[test]
    fn test_parse_memory_bytes_errors() {
        assert!(parse_memory_bytes("").is_err());
        assert!(parse_memory_bytes("abc").is_err());
        assert!(parse_memory_bytes("Mi").is_err());
    }

    #[test]
    fn test_validate_cpu_range() {
        assert!(validate_cpu_range("500m", "100m", "2").is_ok());
        assert!(validate_cpu_range("1", "100m", "2").is_ok());
        assert!(validate_cpu_range("2", "100m", "2").is_ok());
        assert!(validate_cpu_range("100m", "100m", "2").is_ok());

        assert!(validate_cpu_range("50m", "100m", "2").is_err());
        assert!(validate_cpu_range("3", "100m", "2").is_err());
    }

    #[test]
    fn test_validate_memory_range() {
        assert!(validate_memory_range("256Mi", "64Mi", "2Gi").is_ok());
        assert!(validate_memory_range("1Gi", "64Mi", "2Gi").is_ok());
        assert!(validate_memory_range("64Mi", "64Mi", "2Gi").is_ok());
        assert!(validate_memory_range("2Gi", "64Mi", "2Gi").is_ok());

        assert!(validate_memory_range("32Mi", "64Mi", "2Gi").is_err());
        assert!(validate_memory_range("4Gi", "64Mi", "2Gi").is_err());
    }
}
