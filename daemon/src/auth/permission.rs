use regex::Regex;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::sync::{Arc, OnceLock};

// Module for Matchable-related free functions
pub mod matchable {
    use super::*;

    pub fn any(matchables: impl IntoIterator<Item = Arc<dyn Matchable>>) -> Arc<dyn Matchable> {
        Arc::new(AnyMatchable(matchables.into_iter().collect()))
    }

    pub fn all(matchables: impl IntoIterator<Item = Arc<dyn Matchable>>) -> Arc<dyn Matchable> {
        Arc::new(AllMatchable(matchables.into_iter().collect()))
    }

    pub fn always() -> Arc<dyn Matchable> {
        Arc::new(AlwaysMatchable)
    }

    pub fn never() -> Arc<dyn Matchable> {
        Arc::new(NeverMatchable)
    }

    pub fn or(left: Arc<dyn Matchable>, right: Arc<dyn Matchable>) -> Arc<dyn Matchable> {
        Arc::new(OrMatchable { left, right })
    }

    pub fn and(left: Arc<dyn Matchable>, right: Arc<dyn Matchable>) -> Arc<dyn Matchable> {
        Arc::new(AndMatchable { left, right })
    }
}

// Trait equivalent to IMatchable, dyn-compatible and thread-safe
pub trait Matchable: Send + Sync {
    fn matches(&self, other: &dyn Matchable) -> bool;

    // Method to attempt downcasting to Permission
    fn as_permission(&self) -> Option<&Permission> {
        None
    }
}

// Implement Matchable for Arc<dyn Matchable>
impl Matchable for Arc<dyn Matchable> {
    fn matches(&self, other: &dyn Matchable) -> bool {
        (**self).matches(other)
    }

    fn as_permission(&self) -> Option<&Permission> {
        (**self).as_permission()
    }
}

// Structs for composite matchables
struct AnyMatchable(Vec<Arc<dyn Matchable>>);
struct AllMatchable(Vec<Arc<dyn Matchable>>);
struct AlwaysMatchable;
struct NeverMatchable;
struct OrMatchable {
    left: Arc<dyn Matchable>,
    right: Arc<dyn Matchable>,
}
struct AndMatchable {
    left: Arc<dyn Matchable>,
    right: Arc<dyn Matchable>,
}

// Ensure all composite matchables are Send + Sync
unsafe impl Send for AnyMatchable {}
unsafe impl Sync for AnyMatchable {}
unsafe impl Send for AllMatchable {}
unsafe impl Sync for AllMatchable {}
unsafe impl Send for AlwaysMatchable {}
unsafe impl Sync for AlwaysMatchable {}
unsafe impl Send for NeverMatchable {}
unsafe impl Sync for NeverMatchable {}
unsafe impl Send for OrMatchable {}
unsafe impl Sync for OrMatchable {}
unsafe impl Send for AndMatchable {}
unsafe impl Sync for AndMatchable {}

impl Matchable for AnyMatchable {
    fn matches(&self, other: &dyn Matchable) -> bool {
        self.0.iter().any(|m| m.matches(other))
    }
}

impl Matchable for AllMatchable {
    fn matches(&self, other: &dyn Matchable) -> bool {
        self.0.iter().all(|m| m.matches(other))
    }
}

impl Matchable for AlwaysMatchable {
    fn matches(&self, _other: &dyn Matchable) -> bool {
        true
    }
}

impl Matchable for NeverMatchable {
    fn matches(&self, _other: &dyn Matchable) -> bool {
        false
    }
}

impl Matchable for OrMatchable {
    fn matches(&self, other: &dyn Matchable) -> bool {
        self.left.matches(other) || self.right.matches(other)
    }
}

impl Matchable for AndMatchable {
    fn matches(&self, other: &dyn Matchable) -> bool {
        self.left.matches(other) && self.right.matches(other)
    }
}

// Permission struct
#[derive(Clone, Debug)]
pub struct Permission {
    permission: String,
}

impl Permission {
    pub fn new(permission: &str) -> Result<Self, PermissionError> {
        static REGEX: OnceLock<Regex> = OnceLock::new();
        let regex = REGEX.get_or_init(|| {
            Regex::new(r"^((?:[a-zA-Z_-]+|\*{1,2})(?:\.(?:[a-zA-Z_-]+|\*{1,2}))*)$").unwrap()
        });

        if regex.is_match(permission) {
            Ok(Self {
                permission: permission.to_string(),
            })
        } else {
            Err(PermissionError::InvalidPermission(permission.to_string()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.permission
    }
}

impl Matchable for Permission {
    fn matches(&self, other: &dyn Matchable) -> bool {
        if let Some(other_permission) = other.as_permission() {
            let pattern = other_permission
                .permission
                .replace(".", "\\s")
                .replace("**", ".+")
                .replace("*", "\\S+");
            let pattern = format!("^{}(\\s.+)?$", pattern);

            let regex = match Regex::new(&pattern) {
                Ok(regex) => regex,
                Err(_) => return false,
            };

            let input = self.permission.replace(".", " ");
            regex.is_match(&input)
        } else {
            false
        }
    }

    fn as_permission(&self) -> Option<&Permission> {
        Some(self)
    }
}

// Permission error type
#[derive(Debug, thiserror::Error)]
pub enum PermissionError {
    #[error("Invalid permission: {0}")]
    InvalidPermission(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
}

// Serde serialization for Permission
impl Serialize for Permission {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.permission)
    }
}

impl<'de> Deserialize<'de> for Permission {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct PermissionVisitor;

        impl<'de> Visitor<'de> for PermissionVisitor {
            type Value = Permission;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a valid permission string")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Permission::new(value).map_err(|e| E::custom(e.to_string()))
            }
        }

        deserializer.deserialize_str(PermissionVisitor)
    }
}

// Permissions struct
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Permissions {
    #[serde(default)]
    permissions: Vec<Permission>,
}

impl Permissions {
    pub fn new(permissions: impl IntoIterator<Item = Permission>) -> Self {
        Self {
            permissions: permissions.into_iter().collect(),
        }
    }

    pub fn from_str(permissions: &str) -> Result<Self, PermissionError> {
        static REGEX: OnceLock<Regex> = OnceLock::new();
        let regex = REGEX.get_or_init(|| {
            Regex::new(r"(?:(?:[a-zA-Z-_]+|\*{1,2})\.)*(?:[a-zA-Z-_]+|\*{1,2})(?:,(?:(?:[a-zA-Z-_]+|\*{1,2})\.)*(?:[a-zA-Z-_]+|\*{1,2}))*").unwrap()
        });

        if !regex.is_match(permissions) {
            return Err(PermissionError::InvalidPermission(permissions.to_string()));
        }

        let perms = permissions
            .split(',')
            .map(|s| Permission::new(s.trim()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self::new(perms))
    }

    pub fn never() -> Self {
        Self {
            permissions: Vec::new(),
        }
    }

    pub fn permissions(&self) -> &[Permission] {
        &self.permissions
    }
}

impl Matchable for Permissions {
    fn matches(&self, other: &dyn Matchable) -> bool {
        self.permissions.iter().any(|p| p.matches(other))
    }
}

impl fmt::Display for Permissions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let perms = self
            .permissions
            .iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        write!(f, "[{}]", perms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_permission_validity() {
        assert!(Permission::new("user.read").is_ok());
        assert!(Permission::new("user.*").is_ok());
        assert!(Permission::new("user.**").is_ok());
        assert!(Permission::new("*").is_ok());
        assert!(Permission::new("invalid..permission").is_err());
    }

    #[test]
    fn test_permission_matching() {
        let perm1 = Permission::new("user.read").unwrap();
        let perm2 = Permission::new("user.*").unwrap();
        let perm3 = Permission::new("admin.read").unwrap();

        assert!(perm1.matches(&perm2));
        assert!(!perm1.matches(&perm3));
    }

    #[test]
    fn test_permissions_serialization() {
        let perms = Permissions::new(vec![
            Permission::new("user.read").unwrap(),
            Permission::new("user.write").unwrap(),
        ]);
        let json = serde_json::to_string(&perms).unwrap();
        let deserialized: Permissions = serde_json::from_str(&json).unwrap();
        assert_eq!(perms.permissions.len(), deserialized.permissions.len());
    }

    #[test]
    fn test_composite_matchables() {
        let perm1 = Permission::new("user.read").unwrap();
        let perm2 = Permission::new("user.write").unwrap();
        let any = matchable::any(vec![
            Arc::new(perm1.clone()) as Arc<dyn Matchable>,
            Arc::new(perm2.clone()) as Arc<dyn Matchable>,
        ]);
        let all = matchable::all(vec![
            Arc::new(perm1.clone()) as Arc<dyn Matchable>,
            Arc::new(perm2.clone()) as Arc<dyn Matchable>,
        ]);

        assert!(any.matches(&perm1));
        assert!(!all.matches(&perm1)); // All requires both to match
    }

    #[test]
    fn test_thread_safety() {
        let perm = Arc::new(Permission::new("user.read").unwrap());
        let perm_clone = Arc::clone(&perm);
        let handle = std::thread::spawn(move || {
            assert!(perm_clone.matches(&Permission::new("user.*").unwrap()));
        });
        handle.join().unwrap();
    }

    #[test]
    fn test_or_and_combinators() {
        let perm1 = Arc::new(Permission::new("user.read").unwrap());
        let perm2 = Arc::new(Permission::new("user.write").unwrap());
        let or = matchable::or(
            Arc::clone(&perm1) as Arc<dyn Matchable>,
            Arc::clone(&perm2) as Arc<dyn Matchable>,
        );
        let and = matchable::and(
            Arc::clone(&perm1) as Arc<dyn Matchable>,
            Arc::clone(&perm2) as Arc<dyn Matchable>,
        );

        let test_perm = Permission::new("user.read").unwrap();
        assert!(or.matches(&test_perm)); // Matches if either perm1 or perm2 matches
        assert!(!and.matches(&test_perm)); // Matches only if both match
    }
}
