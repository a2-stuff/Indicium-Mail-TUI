use serde::{Deserialize, Serialize};

/// An RFC 5322 mailbox: optional display name + addr-spec.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Address {
    pub name: Option<String>,
    pub email: String,
}

impl Address {
    pub fn new(email: impl Into<String>) -> Self {
        Self { name: None, email: email.into() }
    }

    pub fn named(name: impl Into<String>, email: impl Into<String>) -> Self {
        Self { name: Some(name.into()), email: email.into() }
    }

    /// Render in `Name <email@host>` form (or bare addr if no name).
    pub fn format(&self) -> String {
        match &self.name {
            Some(n) if !n.is_empty() => format!("{} <{}>", n, self.email),
            _ => self.email.clone(),
        }
    }
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.format())
    }
}
