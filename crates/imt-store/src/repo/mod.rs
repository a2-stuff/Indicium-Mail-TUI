//! Repository modules grouping queries by aggregate.

pub mod accounts;
pub mod drafts;
pub mod folders;
pub mod messages;
pub mod search;

use uuid::Uuid;

/// Convert a uuid into the byte form used as sqlite BLOB primary key.
pub(crate) fn uuid_bytes(id: &Uuid) -> Vec<u8> {
    id.as_bytes().to_vec()
}

/// Parse a uuid from a sqlite BLOB column.
pub(crate) fn uuid_from_slice(b: &[u8]) -> Result<Uuid, uuid::Error> {
    Uuid::from_slice(b)
}
