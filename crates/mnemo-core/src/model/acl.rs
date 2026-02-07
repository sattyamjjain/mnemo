use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Acl {
    pub id: Uuid,
    pub memory_id: Uuid,
    pub principal_type: PrincipalType,
    pub principal_id: String,
    pub permission: Permission,
    pub granted_by: String,
    pub created_at: String,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    Read,
    Write,
    Delete,
    Share,
    Delegate,
    Admin,
}

impl std::fmt::Display for Permission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Permission::Read => write!(f, "read"),
            Permission::Write => write!(f, "write"),
            Permission::Delete => write!(f, "delete"),
            Permission::Share => write!(f, "share"),
            Permission::Delegate => write!(f, "delegate"),
            Permission::Admin => write!(f, "admin"),
        }
    }
}

impl std::str::FromStr for Permission {
    type Err = crate::error::Error;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "read" => Ok(Permission::Read),
            "write" => Ok(Permission::Write),
            "delete" => Ok(Permission::Delete),
            "share" => Ok(Permission::Share),
            "delegate" => Ok(Permission::Delegate),
            "admin" => Ok(Permission::Admin),
            _ => Err(crate::error::Error::Validation(format!(
                "invalid permission: {s}"
            ))),
        }
    }
}

impl Permission {
    pub fn satisfies(&self, required: Permission) -> bool {
        // Hierarchy: Admin > Delegate > Share > Delete > Write > Read
        let level = |p: &Permission| -> u8 {
            match p {
                Permission::Read => 0,
                Permission::Write => 1,
                Permission::Delete => 2,
                Permission::Share => 3,
                Permission::Delegate => 4,
                Permission::Admin => 5,
            }
        };
        level(self) >= level(&required)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalType {
    Agent,
    Org,
    Public,
    User,
    Role,
}

impl std::fmt::Display for PrincipalType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PrincipalType::Agent => write!(f, "agent"),
            PrincipalType::Org => write!(f, "org"),
            PrincipalType::Public => write!(f, "public"),
            PrincipalType::User => write!(f, "user"),
            PrincipalType::Role => write!(f, "role"),
        }
    }
}

impl std::str::FromStr for PrincipalType {
    type Err = crate::error::Error;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "agent" => Ok(PrincipalType::Agent),
            "org" => Ok(PrincipalType::Org),
            "public" => Ok(PrincipalType::Public),
            "user" => Ok(PrincipalType::User),
            "role" => Ok(PrincipalType::Role),
            _ => Err(crate::error::Error::Validation(format!(
                "invalid principal type: {s}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acl_serde_roundtrip() {
        let acl = Acl {
            id: Uuid::now_v7(),
            memory_id: Uuid::now_v7(),
            principal_type: PrincipalType::Agent,
            principal_id: "agent-2".to_string(),
            permission: Permission::Read,
            granted_by: "agent-1".to_string(),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            expires_at: None,
        };
        let json = serde_json::to_string(&acl).unwrap();
        let deserialized: Acl = serde_json::from_str(&json).unwrap();
        assert_eq!(acl, deserialized);
    }

    #[test]
    fn test_permission_satisfies() {
        // Admin satisfies everything
        assert!(Permission::Admin.satisfies(Permission::Read));
        assert!(Permission::Admin.satisfies(Permission::Write));
        assert!(Permission::Admin.satisfies(Permission::Delete));
        assert!(Permission::Admin.satisfies(Permission::Share));
        assert!(Permission::Admin.satisfies(Permission::Delegate));
        assert!(Permission::Admin.satisfies(Permission::Admin));
        // Write satisfies Read and Write but not higher
        assert!(Permission::Write.satisfies(Permission::Read));
        assert!(Permission::Write.satisfies(Permission::Write));
        assert!(!Permission::Write.satisfies(Permission::Admin));
        assert!(!Permission::Write.satisfies(Permission::Delete));
        // Read only satisfies Read
        assert!(Permission::Read.satisfies(Permission::Read));
        assert!(!Permission::Read.satisfies(Permission::Write));
        // Delegate satisfies Share and below
        assert!(Permission::Delegate.satisfies(Permission::Share));
        assert!(Permission::Delegate.satisfies(Permission::Delete));
        assert!(!Permission::Delegate.satisfies(Permission::Admin));
    }
}
