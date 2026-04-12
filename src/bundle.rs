use eyre::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, info, warn};

use crate::config::*;

// ============== Generic bundle request (shared schema) ==============

#[derive(Debug, Serialize)]
struct BundleRequest {
    jsonrpc: &'static str,
    id: u32,
    method: &'static str,
    params: Vec<BundleParams>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BundleParams {
    txs: Vec<String>,
    max_block_number: u64,
    max_timestamp: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    reverting_tx_hashes: Vec<String>,
    #[serde(rename = "48spSign", skip_serializing_if = "Option::is_none")]
    sp_sign: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    no_merge: Option<bool>,
}

use alloy::primitives::{keccak256, Address};
use alloy::signers::local::PrivateKeySigner;
use alloy::signers::SignerSync;

#[derive(Debug, Deserialize, Clone)]
pub struct BundleResponse {
    pub result: Option<serde_json::Value>,
    pub error: Option<serde_json::Value>,
}

impl BundleResponse {
    pub fn is_nonce_too_low_for(&self, our_address: Address) -> bool {
        if let Some(err) = &self.error {
            let err_str = err.to_string().to_lowercase();
            let our_addr_lower = format!("{:?}", our_address).to_lowercase();
            (err_str.contains("nonce too low") || err_str.contains("nonce is too low"))
                && err_str.contains(&our_addr_lower)
        } else {
            false
        }
    }

    pub fn is_nonce_too_high_for(&self, our_address: Address) -> bool {
        if let Some(err) = &self.error {
            let err_str = err.to_string().to_lowercase();
            let our_addr_lower = format!("{:?}", our_address).to_lowercase();
            (err_str.contains("nonce too high") || err_str.contains("nonce is too high"))
                && err_str.contains(&our_addr_lower)
        } else {
            false
        }
    }

    pub fn is_nonce_error_for(&self, our_address: Address) -> bool {
        self.is_nonce_too_low_for(our_address) || self.is_nonce_too_high_for(our_address)
    }
}

// ============== Relay enum ==============

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Relay {
    FortyEightClub,
    BlockRazor,
    NodeReal,
}

impl std::fmt::Display for Relay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Relay::FortyEightClub => write!(f, "48Club"),
            Relay::BlockRazor => write!(f, "BlockRazor"),
            Relay::NodeReal => write!(f, "NodeReal"),
        }
    }
}

// ============== Multi-relay bundle sender ==============

pub struct BundleSender {
    client: Client,
    signer: PrivateKeySigner,
    blockrazor_auth: Option<String>,
    nodereal_url: String,
    _48club_url: String,
    blockrazor_url: String,
}

impl BundleSender {
    pub fn new(signer: PrivateKeySigner) -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap_or_else(|_| Client::new()),
            signer,
            blockrazor_auth: BLOCKRAZOR_AUTH_TOKEN.clone(),
            nodereal_url: RELAY_NODEREAL.to_string(),
            _48club_url: RELAY_48CLUB.to_string(),
            blockrazor_url: RELAY_BLOCKRAZOR.to_string(),
        }
    }

    /// Sign bundle for 48Club SoulPoint (gives detailed error messages)
    /// Method: keccak256(concat(keccak256(raw_tx1), keccak256(raw_tx2), ...))
    fn sign_bundle(&self, raw_txs: &[Vec<u8>]) -> Option<String> {
        let mut hashes = Vec::with_capacity(32 * raw_txs.len());
        for tx in raw_txs {
            let hash = keccak256(tx);
            hashes.extend_from_slice(hash.as_slice());
        }
        let msg_hash = keccak256(&hashes);

        match self.signer.sign_hash_sync(&msg_hash) {
            Ok(sig) => {
                let mut sig_bytes = Vec::with_capacity(65);
                sig_bytes.extend_from_slice(&sig.r().to_be_bytes::<32>());
                sig_bytes.extend_from_slice(&sig.s().to_be_bytes::<32>());
                sig_bytes.push(sig.v() as u8);
                Some(format!("0x{}", hex::encode(&sig_bytes)))
            }
            Err(e) => {
                error!("Failed to sign bundle: {}", e);
                None
            }
        }
    }

    /// Build the base bundle params (shared across all relays)
    fn make_params(
        &self,
        signed_txs: &[Vec<u8>],
        max_block: u64,
        reverting_tx_hashes: Vec<String>,
        for_48club: bool,
    ) -> BundleParams {
        let current_ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let txs_hex: Vec<String> = signed_txs
            .iter()
            .map(|tx| format!("0x{}", hex::encode(tx)))
            .collect();

        BundleParams {
            txs: txs_hex,
            max_block_number: max_block,
            max_timestamp: current_ts + *MAX_TIMESTAMP_DELTA,
            reverting_tx_hashes,
            sp_sign: if for_48club { self.sign_bundle(signed_txs) } else { None },
            no_merge: None, // BlockRazor supports it but we don't need it
        }
    }

    /// Dispatch triple-bundle to ALL relays in parallel.
    /// Fire and forget — never blocks.
    ///
    /// Bundle A → block N:     [createToken + buy]  (ideal backrun)
    /// Bundle B → block N:     [buy]                 (standalone, same block)
    /// Bundle C → block N+1:   [buy]                 (standalone, next block)
    ///
    /// 9 requests total: 3 bundles × 3 relays
    pub async fn dispatch_triple_bundle(
        &self,
        create_token_tx: Vec<u8>,
        buy_tx: Vec<u8>,
        current_block: u64,
    ) {
        let next_block = current_block + 1 + *MAX_BLOCK_DELTA;
        let block_n = current_block + *MAX_BLOCK_DELTA;

        // Clone everything we need for the spawned tasks (no borrows across tokio::spawn)
        let client = self.client.clone();
        let signer = self.signer.clone();
        let blockrazor_auth = self.blockrazor_auth.clone();
        let nodereal_url = self.nodereal_url.clone();
        let _48club_url = self._48club_url.clone();
        let blockrazor_url = self.blockrazor_url.clone();

        // Build the 3 bundle payloads
        let bundle_a: Vec<Vec<u8>> = vec![create_token_tx.clone(), buy_tx.clone()];
        let bundle_b: Vec<Vec<u8>> = vec![buy_tx.clone()];
        let bundle_c: Vec<Vec<u8>> = vec![buy_tx.clone()];

        // Launch all 9 requests in parallel (3 relay tasks, each fires 3 bundles)
        let mut handles = Vec::with_capacity(3);

        for relay in [Relay::FortyEightClub, Relay::BlockRazor, Relay::NodeReal] {
            let client = client.clone();
            let signer = signer.clone();
            let ba = bundle_a.clone();
            let bb = bundle_b.clone();
            let bc = bundle_c.clone();
            let auth = blockrazor_auth.clone();
            let nr_url = nodereal_url.clone();
            let fc_url = _48club_url.clone();
            let br_url = blockrazor_url.clone();

            let h = tokio::spawn(async move {
                let a = Self::send_single_bundle(
                    &client, &signer, relay, ba, block_n, vec![],
                    "A[create+buy]", &fc_url, &br_url, &nr_url, auth.as_deref(),
                ).await;
                let b = Self::send_single_bundle(
                    &client, &signer, relay, bb, block_n, vec![],
                    "B[buy]", &fc_url, &br_url, &nr_url, auth.as_deref(),
                ).await;
                let c = Self::send_single_bundle(
                    &client, &signer, relay, bc, next_block, vec![],
                    "C[buy]", &fc_url, &br_url, &nr_url, auth.as_deref(),
                ).await;
                (relay, a, b, c)
            });
            handles.push(h);
        }

        // Wait for all relay groups to finish
        for handle in handles {
            if let Err(e) = handle.await {
                warn!("Relay dispatch task panicked: {}", e);
            }
        }
    }

    /// Send a single bundle to one relay. Fire and forget.
    async fn send_single_bundle(
        client: &Client,
        signer: &PrivateKeySigner,
        relay: Relay,
        signed_txs: Vec<Vec<u8>>,
        max_block: u64,
        reverting_tx_hashes: Vec<String>,
        label: &str,
        fc_url: &str,
        br_url: &str,
        nr_url: &str,
        auth: Option<&str>,
    ) -> Option<BundleResponse> {
        let (url, sp_sign) = match relay {
            Relay::FortyEightClub => {
                let mut hashes = Vec::with_capacity(32 * signed_txs.len());
                for tx in &signed_txs {
                    hashes.extend_from_slice(keccak256(tx).as_slice());
                }
                let msg_hash = keccak256(&hashes);
                let sp = signer.sign_hash_sync(&msg_hash).ok().map(|sig| {
                    let mut bytes = Vec::with_capacity(65);
                    bytes.extend_from_slice(&sig.r().to_be_bytes::<32>());
                    bytes.extend_from_slice(&sig.s().to_be_bytes::<32>());
                    bytes.push(sig.v() as u8);
                    format!("0x{}", hex::encode(&bytes))
                });
                (fc_url, sp)
            }
            Relay::BlockRazor => (br_url, None),
            Relay::NodeReal => (nr_url, None),
        };

        let current_ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let txs_hex: Vec<String> = signed_txs
            .iter()
            .map(|tx| format!("0x{}", hex::encode(tx)))
            .collect();

        let params = BundleParams {
            txs: txs_hex,
            max_block_number: max_block,
            max_timestamp: current_ts + *MAX_TIMESTAMP_DELTA,
            reverting_tx_hashes,
            sp_sign,
            no_merge: None,
        };

        let request = BundleRequest {
            jsonrpc: "2.0",
            id: 1,
            method: "eth_sendBundle",
            params: vec![params],
        };

        let mut req = client.post(url).json(&request);
        if let Some(token) = auth {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        match req.send().await {
            Ok(resp) => match resp.json::<BundleResponse>().await {
                Ok(result) => {
                    if result.result.is_some() {
                        info!(
                            "✅ Bundle accepted | {} | {} | block {}",
                            relay, label, max_block
                        );
                    } else {
                        warn!(
                            "❌ Bundle rejected | {} | {} | block {} | {:?}",
                            relay, label, max_block, result.error
                        );
                    }
                    Some(result)
                }
                Err(e) => {
                    warn!("Failed to parse response | {} | {} | {}", relay, label, e);
                    None
                }
            },
            Err(e) => {
                warn!("Failed to send | {} | {} | {}", relay, label, e);
                None
            }
        }
    }

    // ==================== Frontrun bundles (single relay fallback) ====================
    // Frontrun bundles only go to 48Club (they need the createToken dependency
    // and frontrun semantics that other relays may not support well).

    /// Send a raw bundle of signed transactions (48Club only).
    pub async fn send_bundle(
        &self,
        signed_txs: Vec<Vec<u8>>,
        current_block: u64,
    ) -> Result<BundleResponse> {
        self.send_bundle_with_reverting(signed_txs, current_block, vec![]).await
    }

    pub async fn send_bundle_with_reverting(
        &self,
        signed_txs: Vec<Vec<u8>>,
        current_block: u64,
        reverting_tx_hashes: Vec<String>,
    ) -> Result<BundleResponse> {
        // Relays require maxBlockNumber > current_block, so we add MAX_BLOCK_DELTA
        let max_block = current_block + *MAX_BLOCK_DELTA;
        let params = self.make_params(&signed_txs, max_block, reverting_tx_hashes, true);
        let request = BundleRequest {
            jsonrpc: "2.0",
            id: 1,
            method: "eth_sendBundle",
            params: vec![params],
        };

        let response = self
            .client
            .post(&self._48club_url)
            .json(&request)
            .send()
            .await?;

        let result: BundleResponse = response.json().await?;

        if result.result.is_some() {
            info!("Bundle accepted: {:?}", result.result);
        } else if result.error.is_some() {
            error!("Bundle rejected: {:?}", result.error);
        }

        Ok(result)
    }

    pub async fn send_bundle_to_block(
        &self,
        signed_txs: Vec<Vec<u8>>,
        max_block: u64,
    ) -> Result<BundleResponse> {
        self.send_bundle_with_reverting_to_block(signed_txs, max_block, vec![])
            .await
    }

    pub async fn send_bundle_with_reverting_to_block(
        &self,
        signed_txs: Vec<Vec<u8>>,
        max_block: u64,
        reverting_tx_hashes: Vec<String>,
    ) -> Result<BundleResponse> {
        let params = self.make_params(&signed_txs, max_block, reverting_tx_hashes, true);
        let request = BundleRequest {
            jsonrpc: "2.0",
            id: 1,
            method: "eth_sendBundle",
            params: vec![params],
        };

        let response = self
            .client
            .post(&self._48club_url)
            .json(&request)
            .send()
            .await?;

        let result: BundleResponse = response.json().await?;

        if result.result.is_some() {
            info!("Bundle accepted (block {}): {:?}", max_block, result.result);
        } else if result.error.is_some() {
            error!("Bundle rejected: {:?}", result.error);
        }

        Ok(result)
    }

    pub async fn send_frontrun_bundle(
        &self,
        our_txs: Vec<Vec<u8>>,
        target_tx: Vec<u8>,
        current_block: u64,
    ) -> Result<BundleResponse> {
        let mut all_txs = our_txs;
        all_txs.push(target_tx);
        self.send_bundle_with_reverting(all_txs, current_block, vec![])
            .await
    }
}
