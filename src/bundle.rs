use eyre::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, info};

use crate::config::*;

// Puissant bundle request (eth_sendBundle)
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
}

use alloy::primitives::{keccak256, Address};
use alloy::signers::k256::ecdsa::SigningKey;
use alloy::signers::local::PrivateKeySigner;
use alloy::signers::{Signer, SignerSync};

#[derive(Debug, Deserialize)]
pub struct BundleResponse {
    pub result: Option<serde_json::Value>,
    pub error: Option<serde_json::Value>,
}

impl BundleResponse {
    /// Check if error is "nonce too low" for a specific address
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

    /// Check if error is "nonce too high" for a specific address
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

    /// Check if error is any nonce problem for our address
    pub fn is_nonce_error_for(&self, our_address: Address) -> bool {
        self.is_nonce_too_low_for(our_address) || self.is_nonce_too_high_for(our_address)
    }
}

/// Bundle sender for 48Club Puissant MEV relay (FREE)
/// Supports: eth_sendBundle for backrun and frontrun operations
pub struct BundleSender {
    client: Client,
    rpc_url: String,
    signer: PrivateKeySigner,
}

impl BundleSender {
    pub fn new(signer: PrivateKeySigner) -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap_or_else(|_| Client::new()),
            rpc_url: PUISSANT_RPC.to_string(),
            signer,
        }
    }

    /// Sign bundle for 48Club SoulPoint (gives detailed error messages)
    /// Method: keccak256(concat(keccak256(raw_tx1), keccak256(raw_tx2), ...))
    fn sign_bundle(&self, raw_txs: &[Vec<u8>]) -> Option<String> {
        use alloy::primitives::B256;

        // Concatenate tx hashes
        let mut hashes = Vec::with_capacity(32 * raw_txs.len());
        for tx in raw_txs {
            let hash = keccak256(tx);
            hashes.extend_from_slice(hash.as_slice());
        }

        // Hash the concatenation
        let msg_hash = keccak256(&hashes);

        // Sign with our private key (synchronous sign_hash)
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

    /// Send a raw bundle of signed transactions
    pub async fn send_bundle(
        &self,
        signed_txs: Vec<Vec<u8>>,
        current_block: u64,
    ) -> Result<BundleResponse> {
        self.send_bundle_with_reverting(signed_txs, current_block, vec![]).await
    }

    /// Send a raw bundle targeting a specific max block number.
    /// Unlike `send_bundle` (which uses current_block + MAX_BLOCK_DELTA),
    /// this lets you control the exact target block.
    pub async fn send_bundle_to_block(
        &self,
        signed_txs: Vec<Vec<u8>>,
        max_block: u64,
    ) -> Result<BundleResponse> {
        self.send_bundle_with_reverting_to_block(signed_txs, max_block, vec![]).await
    }

    /// Send a raw bundle with optional reverting tx hashes
    pub async fn send_bundle_with_reverting(
        &self,
        signed_txs: Vec<Vec<u8>>,
        current_block: u64,
        reverting_tx_hashes: Vec<String>,
    ) -> Result<BundleResponse> {
        let current_ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

        // Sign the bundle for 48SP error details
        let sp_sign = self.sign_bundle(&signed_txs);

        let txs_hex: Vec<String> = signed_txs
            .iter()
            .map(|tx| format!("0x{}", hex::encode(tx)))
            .collect();

        let request = BundleRequest {
            jsonrpc: "2.0",
            id: 1,
            method: "eth_sendBundle",
            params: vec![BundleParams {
                txs: txs_hex,
                max_block_number: current_block + *MAX_BLOCK_DELTA,
                max_timestamp: current_ts + *MAX_TIMESTAMP_DELTA,
                reverting_tx_hashes,
                sp_sign,
            }],
        };

        let response = self
            .client
            .post(&self.rpc_url)
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

    /// Send a raw bundle with optional reverting tx hashes, targeting a specific block.
    pub async fn send_bundle_with_reverting_to_block(
        &self,
        signed_txs: Vec<Vec<u8>>,
        max_block: u64,
        reverting_tx_hashes: Vec<String>,
    ) -> Result<BundleResponse> {
        let current_ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

        // Sign the bundle for 48SP error details
        let sp_sign = self.sign_bundle(&signed_txs);

        let txs_hex: Vec<String> = signed_txs
            .iter()
            .map(|tx| format!("0x{}", hex::encode(tx)))
            .collect();

        let request = BundleRequest {
            jsonrpc: "2.0",
            id: 1,
            method: "eth_sendBundle",
            params: vec![BundleParams {
                txs: txs_hex,
                max_block_number: max_block,
                max_timestamp: current_ts + *MAX_TIMESTAMP_DELTA,
                reverting_tx_hashes,
                sp_sign,
            }],
        };

        let response = self
            .client
            .post(&self.rpc_url)
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

    /// Send backrun bundle: [target_tx, our_txs...]
    /// Our transactions execute AFTER the target transaction in the same block
    pub async fn send_backrun_bundle(
        &self,
        target_tx: Vec<u8>,
        our_txs: Vec<Vec<u8>>,
        current_block: u64,
    ) -> Result<BundleResponse> {
        let mut all_txs = vec![target_tx];
        all_txs.extend(our_txs);
        self.send_bundle(all_txs, current_block).await
    }

    /// Send frontrun bundle: [our_txs..., target_tx]
    /// Our transactions execute BEFORE the target transaction in the same block
    pub async fn send_frontrun_bundle(
        &self,
        our_txs: Vec<Vec<u8>>,
        target_tx: Vec<u8>,
        current_block: u64,
    ) -> Result<BundleResponse> {
        let mut all_txs = our_txs;
        all_txs.push(target_tx);
        self.send_bundle(all_txs, current_block).await
    }
}
