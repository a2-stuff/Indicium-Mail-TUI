use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Flag {
    Seen,
    Answered,
    Flagged,
    Deleted,
    Draft,
    Recent,
    Custom(String),
}

impl Flag {
    pub fn as_imap_str(&self) -> &str {
        match self {
            Flag::Seen => "\\Seen",
            Flag::Answered => "\\Answered",
            Flag::Flagged => "\\Flagged",
            Flag::Deleted => "\\Deleted",
            Flag::Draft => "\\Draft",
            Flag::Recent => "\\Recent",
            Flag::Custom(s) => s.as_str(),
        }
    }
}
