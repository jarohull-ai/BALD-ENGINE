use sha2::{Digest, Sha256};

pub fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn sha256_prefixed(input: &str) -> String {
    format!("sha256:{}", sha256_hex(input))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_hash() {
        assert_eq!(sha256_hex("bald"), sha256_hex("bald"));
        assert_ne!(sha256_hex("bald"), sha256_hex("BALD"));
        assert_eq!(sha256_hex("bald").len(), 64);
    }
}
