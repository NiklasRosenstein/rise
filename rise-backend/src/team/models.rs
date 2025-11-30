use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashSet;

// Custom deserializer that handles empty strings as empty arrays
fn deserialize_string_or_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::{self, Visitor};
    use std::fmt;

    struct StringOrVec;

    impl<'de> Visitor<'de> for StringOrVec {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string or array of strings")
        }

        fn visit_str<E>(self, value: &str) -> Result<Vec<String>, E>
        where
            E: de::Error,
        {
            // Empty string becomes empty vec, non-empty string becomes vec with one element
            if value.is_empty() {
                Ok(Vec::new())
            } else {
                Ok(vec![value.to_string()])
            }
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Vec<String>, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            let mut vec = Vec::new();
            while let Some(value) = seq.next_element()? {
                vec.push(value);
            }
            Ok(vec)
        }
    }

    deserializer.deserialize_any(StringOrVec)
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Team {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, deserialize_with = "deserialize_string_or_vec")]
    pub members: Vec<String>,  // User IDs
    #[serde(default)]
    pub owners: Vec<String>,   // User IDs
    // PocketBase system fields
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub updated: String,
    #[serde(default)]
    pub collectionId: String,
    #[serde(default)]
    pub collectionName: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CreateTeamRequest {
    pub name: String,
    pub members: Vec<String>,  // User IDs to add as members
    pub owners: Vec<String>,   // User IDs to add as owners (must include authenticated user)
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CreateTeamResponse {
    pub team: Team,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UpdateTeamRequest {
    pub name: Option<String>,
    pub members: Option<Vec<String>>,
    pub owners: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UpdateTeamResponse {
    pub team: Team,
}

// User information for expanded team responses
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
}

// Team with expanded user information (emails instead of just IDs)
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TeamWithEmails {
    pub id: String,
    pub name: String,
    pub members: Vec<UserInfo>,
    pub owners: Vec<UserInfo>,
    pub created: String,
    pub updated: String,
    #[serde(rename = "collectionId")]
    pub collection_id: String,
    #[serde(rename = "collectionName")]
    pub collection_name: String,
}

// Error response with optional fuzzy match suggestions
#[derive(Debug, Serialize, Clone)]
pub struct TeamErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestions: Option<Vec<String>>,
}

// Query parameters for team lookup
#[derive(Debug, Deserialize, Clone)]
pub struct GetTeamParams {
    #[serde(default)]
    pub by_id: bool,
    #[serde(default)]
    pub expand: String,  // Comma-separated list like "members,owners"
}

impl GetTeamParams {
    /// Check if a field should be expanded
    pub fn should_expand(&self, field: &str) -> bool {
        if self.expand.is_empty() {
            return false;
        }

        let fields: HashSet<&str> = self.expand.split(',').map(|s| s.trim()).collect();
        fields.contains(field)
    }
}
