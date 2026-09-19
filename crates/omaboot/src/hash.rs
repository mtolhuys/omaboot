//! Content hashing.
//!
//! The rollback record and the verify step both compare digests rather than
//! whole files, so `omaboot status` can report drift without reading the theme
//! twice.

use sha2::{Digest, Sha256};

/// Lowercase hexadecimal SHA-256 of a byte slice.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// A digest over an ordered set of named contents, used as the theme hash.
///
/// Names are included in the digest, so renaming a file changes the hash, and
/// the caller is responsible for passing the entries in a stable order.
pub fn sha256_manifest<'a>(entries: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> String {
    let mut hasher = Sha256::new();
    for (name, bytes) in entries {
        hasher.update((name.len() as u64).to_le_bytes());
        hasher.update(name.as_bytes());
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    }
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn manifest_is_order_sensitive_and_name_sensitive() {
        let a = sha256_manifest([("a", &b"1"[..]), ("b", &b"2"[..])]);
        let reordered = sha256_manifest([("b", &b"2"[..]), ("a", &b"1"[..])]);
        let renamed = sha256_manifest([("a", &b"1"[..]), ("c", &b"2"[..])]);
        assert_ne!(a, reordered);
        assert_ne!(a, renamed);
        assert_eq!(a, sha256_manifest([("a", &b"1"[..]), ("b", &b"2"[..])]));
    }

    #[test]
    fn manifest_separates_fields() {
        // Without length prefixes these two would collide.
        let one = sha256_manifest([("ab", &b"c"[..])]);
        let two = sha256_manifest([("a", &b"bc"[..])]);
        assert_ne!(one, two);
    }
}
