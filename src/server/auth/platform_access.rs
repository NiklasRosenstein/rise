//! Platform access control
//!
//! Determines whether a user has access to Rise platform features (CLI/API/Dashboard)
//! vs application-only access (ingress auth).

use crate::db::models::User;
use crate::server::settings::{PlatformAccessConfig, PlatformAccessPolicy};

/// Trait for checking platform access permissions
pub trait PlatformAccessChecker {
    /// Check if a user has platform access based on email and IdP groups
    fn has_platform_access(&self, user: &User, idp_groups: Option<&[String]>) -> bool;
}

/// Config-based platform access checker
pub struct ConfigBasedAccessChecker<'a> {
    pub config: &'a PlatformAccessConfig,
    pub admin_users: &'a [String],
}

impl<'a> PlatformAccessChecker for ConfigBasedAccessChecker<'a> {
    fn has_platform_access(&self, user: &User, idp_groups: Option<&[String]>) -> bool {
        // Admin users always have platform access
        if super::admin::is_admin_user(self.admin_users, &user.email) {
            return true;
        }

        match self.config.policy {
            PlatformAccessPolicy::AllowAll => true,
            PlatformAccessPolicy::Restrictive => {
                // Check email allowlist
                if self
                    .config
                    .allowed_user_emails
                    .iter()
                    .any(|email| email.eq_ignore_ascii_case(&user.email))
                {
                    return true;
                }

                // Check IdP group allowlist
                if let Some(groups) = idp_groups {
                    if self.config.allowed_idp_groups.iter().any(|allowed_group| {
                        groups
                            .iter()
                            .any(|user_group| user_group.eq_ignore_ascii_case(allowed_group))
                    }) {
                        return true;
                    }
                }

                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn make_user(email: &str) -> User {
        User {
            id: Uuid::new_v4(),
            email: email.to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_allow_all_policy() {
        let config = PlatformAccessConfig {
            policy: PlatformAccessPolicy::AllowAll,
            allowed_user_emails: vec![],
            allowed_idp_groups: vec![],
        };
        let checker = ConfigBasedAccessChecker {
            config: &config,
            admin_users: &[],
        };

        let user = make_user("anyone@example.com");
        assert!(checker.has_platform_access(&user, None));
    }

    #[test]
    fn test_admin_always_has_access() {
        let config = PlatformAccessConfig {
            policy: PlatformAccessPolicy::Restrictive,
            allowed_user_emails: vec![],
            allowed_idp_groups: vec![],
        };
        let checker = ConfigBasedAccessChecker {
            config: &config,
            admin_users: &["admin@example.com".to_string()],
        };

        let user = make_user("admin@example.com");
        assert!(checker.has_platform_access(&user, None));
    }

    #[test]
    fn test_restrictive_email_allowlist() {
        let config = PlatformAccessConfig {
            policy: PlatformAccessPolicy::Restrictive,
            allowed_user_emails: vec!["allowed@example.com".to_string()],
            allowed_idp_groups: vec![],
        };
        let checker = ConfigBasedAccessChecker {
            config: &config,
            admin_users: &[],
        };

        let allowed_user = make_user("allowed@example.com");
        assert!(checker.has_platform_access(&allowed_user, None));

        let denied_user = make_user("denied@example.com");
        assert!(!checker.has_platform_access(&denied_user, None));
    }

    #[test]
    fn test_restrictive_group_allowlist() {
        let config = PlatformAccessConfig {
            policy: PlatformAccessPolicy::Restrictive,
            allowed_user_emails: vec![],
            allowed_idp_groups: vec!["engineering".to_string()],
        };
        let checker = ConfigBasedAccessChecker {
            config: &config,
            admin_users: &[],
        };

        let user = make_user("user@example.com");

        // User with allowed group
        assert!(checker.has_platform_access(&user, Some(&["engineering".to_string()])));

        // User without allowed group
        assert!(!checker.has_platform_access(&user, Some(&["marketing".to_string()])));

        // User with no groups
        assert!(!checker.has_platform_access(&user, None));
    }

    #[test]
    fn test_case_insensitive_matching() {
        let config = PlatformAccessConfig {
            policy: PlatformAccessPolicy::Restrictive,
            allowed_user_emails: vec!["User@Example.Com".to_string()],
            allowed_idp_groups: vec!["Engineering".to_string()],
        };
        let checker = ConfigBasedAccessChecker {
            config: &config,
            admin_users: &["Admin@Example.Com".to_string()],
        };

        // Email case insensitive
        let user1 = make_user("user@example.com");
        assert!(checker.has_platform_access(&user1, None));

        // Admin case insensitive
        let user2 = make_user("admin@example.com");
        assert!(checker.has_platform_access(&user2, None));

        // Group case insensitive
        let user3 = make_user("other@example.com");
        assert!(checker.has_platform_access(&user3, Some(&["engineering".to_string()])));
    }
}
