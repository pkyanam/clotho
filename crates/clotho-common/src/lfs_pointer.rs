//! Git-LFS-compatible pointer contract for payloads stored by Arachne.

use sha2::{Digest, Sha256};

pub const LFS_VERSION: &str = "https://git-lfs.github.com/spec/v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LfsPointer {
    /// Standard git-lfs object id: SHA-256 of the original payload.
    pub oid_sha256: String,
    pub size: u64,
    /// Clotho extension: Arachne Merkle file hash used for reconstruction.
    pub arachne_hash: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PointerError {
    #[error("not a git-lfs pointer")]
    NotPointer,
    #[error("invalid git-lfs pointer: {0}")]
    Invalid(String),
}

impl LfsPointer {
    pub fn for_payload(payload: &[u8], arachne_hash: impl Into<String>) -> Self {
        Self {
            oid_sha256: format!("{:x}", Sha256::digest(payload)),
            size: payload.len() as u64,
            arachne_hash: arachne_hash.into(),
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        format!(
            "version {LFS_VERSION}\noid sha256:{}\nsize {}\nx-clotho-arachne-hash {}\n",
            self.oid_sha256, self.size, self.arachne_hash
        )
        .into_bytes()
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, PointerError> {
        let text = std::str::from_utf8(bytes).map_err(|_| PointerError::NotPointer)?;
        let mut lines = text.lines();
        let expected_version = format!("version {LFS_VERSION}");
        if lines.next() != Some(expected_version.as_str()) {
            return Err(PointerError::NotPointer);
        }
        let mut oid = None;
        let mut size = None;
        let mut arachne_hash = None;
        for line in lines {
            if let Some(value) = line.strip_prefix("oid sha256:") {
                oid = Some(valid_hex(value, "oid")?);
            } else if let Some(value) = line.strip_prefix("size ") {
                size = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| PointerError::Invalid("size must be an integer".into()))?,
                );
            } else if let Some(value) = line.strip_prefix("x-clotho-arachne-hash ") {
                arachne_hash = Some(valid_hex(value, "Arachne hash")?);
            }
        }
        Ok(Self {
            oid_sha256: oid.ok_or_else(|| PointerError::Invalid("missing oid".into()))?,
            size: size.ok_or_else(|| PointerError::Invalid("missing size".into()))?,
            arachne_hash: arachne_hash
                .ok_or_else(|| PointerError::Invalid("missing Arachne hash".into()))?,
        })
    }

    pub fn verify_payload(&self, payload: &[u8]) -> Result<(), PointerError> {
        if payload.len() as u64 != self.size {
            return Err(PointerError::Invalid("materialized size mismatch".into()));
        }
        let actual = format!("{:x}", Sha256::digest(payload));
        if actual != self.oid_sha256 {
            return Err(PointerError::Invalid(
                "materialized SHA-256 mismatch".into(),
            ));
        }
        Ok(())
    }
}

fn valid_hex(value: &str, field: &str) -> Result<String, PointerError> {
    if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(PointerError::Invalid(format!(
            "{field} must be 64 hexadecimal characters"
        )));
    }
    Ok(value.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_verify() {
        let data = b"model payload";
        let pointer = LfsPointer::for_payload(data, "a".repeat(64));
        let parsed = LfsPointer::parse(&pointer.encode()).unwrap();
        assert_eq!(parsed, pointer);
        parsed.verify_payload(data).unwrap();
    }

    #[test]
    fn rejects_plain_files_and_corrupt_payloads() {
        assert_eq!(LfsPointer::parse(b"hello"), Err(PointerError::NotPointer));
        let pointer = LfsPointer::for_payload(b"hello", "b".repeat(64));
        assert!(pointer.verify_payload(b"world").is_err());
    }
}
