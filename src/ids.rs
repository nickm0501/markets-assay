use anyhow::Result;
use serde::Serialize;
use sha2::{Digest, Sha256};

pub fn stable_id(prefix: &str, value: &impl Serialize) -> Result<String> {
    let bytes = serde_json::to_vec(value)?;
    let digest = Sha256::digest(bytes);
    let hex = hex::encode(digest);
    Ok(format!("{prefix}_{}", &hex[..16]))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_id_uses_canonical_json() {
        #[derive(serde::Serialize)]
        struct Sample {
            symbol: &'static str,
            value: i32,
        }

        let first = stable_id(
            "ds",
            &Sample {
                symbol: "SPY",
                value: 7,
            },
        )
        .unwrap();
        let second = stable_id(
            "ds",
            &Sample {
                symbol: "SPY",
                value: 7,
            },
        )
        .unwrap();

        assert_eq!(first, second);
        assert!(first.starts_with("ds_"));
        assert_eq!(first.len(), 19);
    }
}
