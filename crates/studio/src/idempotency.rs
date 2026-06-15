//! Idempotency keys for produced outputs. Keying on the content hash (not just
//! the range) means an upstream chapter edit yields a new key, so corrected
//! chapters re-render while unchanged ranges are skipped.

use sha2::{Digest, Sha256};

/// SHA-256 (hex) over an ordered list of strings, length-delimited.
pub fn content_hash(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p.len().to_string().as_bytes());
        h.update([0u8]);
        h.update(p.as_bytes());
    }
    hex(&h.finalize())
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

pub struct OutputKey<'a> {
    pub novel_slug: &'a str,
    pub first: u32,
    pub last: u32,
    pub workflow_version: u32,
    pub hash: &'a str,
}

/// A stable key for `(novel, chapter range, workflow version, content)`.
pub fn output_key(k: &OutputKey) -> String {
    format!(
        "{}:{}-{}:v{}:{}",
        k.novel_slug,
        k.first,
        k.last,
        k.workflow_version,
        &k.hash[..k.hash.len().min(12)]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_stable_and_order_sensitive() {
        assert_eq!(content_hash(&["a", "b"]), content_hash(&["a", "b"]));
        assert_ne!(content_hash(&["a", "b"]), content_hash(&["b", "a"]));
    }

    #[test]
    fn key_encodes_fields() {
        let h = content_hash(&["x"]);
        let k = output_key(&OutputKey {
            novel_slug: "yeu-than-ky",
            first: 1,
            last: 5,
            workflow_version: 2,
            hash: &h,
        });
        assert!(k.starts_with("yeu-than-ky:1-5:v2:"));
    }

    #[test]
    fn key_changes_with_content() {
        let a = output_key(&OutputKey {
            novel_slug: "s",
            first: 1,
            last: 3,
            workflow_version: 1,
            hash: &content_hash(&["v1"]),
        });
        let b = output_key(&OutputKey {
            novel_slug: "s",
            first: 1,
            last: 3,
            workflow_version: 1,
            hash: &content_hash(&["v2"]),
        });
        assert_ne!(a, b);
    }
}
