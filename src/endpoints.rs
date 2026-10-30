use std::sync::OnceLock;

use serde::Deserialize;
use sha2::{Digest, Sha256};

const SEALED: &[u8] = include_bytes!("../endpoints.dat");
const SEED: &[u8] = b"primeboost-astra-endpoints/1";

#[derive(Deserialize)]
struct Endpoints {
    site_url: String,
    site_api: String,
}

static ENDPOINTS: OnceLock<Endpoints> = OnceLock::new();

fn unseal(data: &[u8]) -> Vec<u8> {
    data.chunks(32)
        .enumerate()
        .flat_map(|(index, chunk)| {
            let mut hasher = Sha256::new();
            hasher.update(SEED);
            hasher.update((index as u64).to_le_bytes());
            let block = hasher.finalize();
            chunk.iter().zip(block).map(|(byte, key)| byte ^ key).collect::<Vec<_>>()
        })
        .collect()
}

fn endpoints() -> &'static Endpoints {
    ENDPOINTS.get_or_init(|| {
        serde_json::from_slice(&unseal(SEALED)).expect("endpoints.dat is invalid; run scripts/seal-endpoints.py")
    })
}

pub fn site_url() -> &'static str {
    &endpoints().site_url
}

pub fn site_api() -> &'static str {
    &endpoints().site_api
}

#[cfg(test)]
mod tests {
    #[test]
    fn sealed_endpoints_decode() {
        assert!(super::site_api().starts_with(super::site_url()));
    }
}
