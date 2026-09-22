use serde::{Deserialize, Serialize};
use std::fmt;
use sha2::{Sha256, Digest};

pub fn hash_token_str(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenRole {
    Admin,
    Write,
    Read,
}

impl TokenRole {
    pub fn has_write(&self) -> bool {
        matches!(self, TokenRole::Admin | TokenRole::Write)
    }

    pub fn has_admin(&self) -> bool {
        matches!(self, TokenRole::Admin)
    }
}

impl fmt::Display for TokenRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenRole::Admin => write!(f, "admin"),
            TokenRole::Write => write!(f, "write"),
            TokenRole::Read => write!(f, "read"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiToken {
    pub id: String,
    pub name: String,
    pub key_hash: String,
    pub role: TokenRole,
    pub created: String,
    pub expires: Option<String>,
}

impl ApiToken {
    pub fn has_write(&self) -> bool {
        self.role.has_write()
    }

    pub fn has_admin(&self) -> bool {
        self.role.has_admin()
    }
}
