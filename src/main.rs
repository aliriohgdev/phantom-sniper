mod bundle;
mod config;
mod contracts;
mod decoder;
mod trader;

use alloy::consensus::Transaction as _;
use alloy::network::TransactionResponse as _;
use alloy::primitives::{Address, B256, U256};
use alloy::providers::{Provider, ProviderBuilder, WsConnect, IpcConnect};
use dashmap::{DashMap, DashSet};
use eyre::Result;
use futures::StreamExt;
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn, Level};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt};

use bundle::BundleSender;
use config::*;
use decoder::{decode_create_token_calldata, predict_token_address};
use trader::Trader;

#[derive(Clone)]
struct TokenInfo {
    token: Address,
    developer: Address,
    our_position: U256,        // current token balance
    cost_basis_bnb: U256,      // BNB we spent to buy
    peak_value_bnb: U256,      // highest BNB value seen (trailing stop-loss anchor)
    last_value_bnb: U256,      // last known trySell value (for stagnation detection)
    last_value_update: std::time::Instant, // last time value changed
    dev_initial_balance: U256, // dev token balance right after creation
    dev_cumulative_sold: U256, // total tokens dev has sold so far
    created_at: std::time::Instant, // when we entered this position
}

type TokenMemory = Arc<DashMap<Address, TokenInfo>>; // token -> info

#[derive(Debug, PartialEq, Clone, Copy)]
enum TxType {
    CreateToken,
    BuyToken,
    SellToken,
    Unknown,
}

fn categorize_tx(input: &str) -> TxType {
    // IMPORTANT: Use full 10-char selector comparison to avoid partial matches
    // Check BuyToken before CreateToken to prevent overlap issues

    if input.len() < 10 {
        return TxType::Unknown;
    }

    let selector = &input[..10];

    if selector == BUY_TOKEN_SELECTOR_1
        || selector == BUY_TOKEN_SELECTOR_2
        || selector == BUY_TOKEN_SELECTOR_3
    {
        TxType::BuyToken
    } else if selector == CREATE_TOKEN_SELECTOR {
        TxType::CreateToken
    } else if selector == SELL_TOKEN_SELECTOR_1
        || selector == SELL_TOKEN_SELECTOR_2
        || selector == SELL_TOKEN_SELECTOR_3
        || selector == SELL_TOKEN_SELECTOR_4
    {
        TxType::SellToken
    } else {
        TxType::Unknown
    }
}

/// Extract token address and amount from sellToken calldata
/// Handles multiple sellToken signatures:
/// - 0x3e11741f: sellToken(address,uint256,uint256) - token is 1st param
/// - 0xf464e7db: sellToken(address,uint256) - token is 1st param
/// - 0x06e7b98f: sellToken(uint256,address,uint256,uint256,uint256,address) - token is 2nd param
/// - 0x0da74935: sellToken(uint256,address,uint256,uint256) - token is 2nd param
fn extract_token_from_sell(input: &[u8]) -> Option<(Address, U256)> {
    if input.len() < 68 {
        return None;
    }

    let selector = &input[..4];
    let selector_hex = format!(
        "0x{:02x}{:02x}{:02x}{:02x}",
        selector[0], selector[1], selector[2], selector[3]
    );

    // Check if token is second param (uint256 first)
    let token_is_second_param =
        selector_hex == SELL_TOKEN_SELECTOR_3 || selector_hex == SELL_TOKEN_SELECTOR_4;

    if token_is_second_param {
        // sellToken(uint256, address, ...) - token at bytes 48-68
        if input.len() < 100 {
            return None;
        }
        let token = Address::from_slice(&input[48..68]); // 4 selector + 32 uint256 + 12 padding
        let amount = U256::from_be_slice(&input[68..100]); // amount is 3rd param
        Some((token, amount))
    } else {
        // sellToken(address, uint256, ...) - token at bytes 16-36
        let token = Address::from_slice(&input[16..36]); // 4 selector + 12 padding
        let amount = U256::from_be_slice(&input[36..68]);
        Some((token, amount))
    }
}

/// Check if raw input data contains a 20-byte address (ABI-encoded as 32-byte word with 12-byte zero prefix)
/// Align token amount to gwei precision (zero out last 9 digits).
/// four.meme sellToken reverts with "GW" if the amount has non-zero digits in the last 9 decimals.
fn align_to_gwei(amount: U256) -> U256 {
    let gwei = U256::from(1_000_000_000u64); // 1e9
    (amount / gwei) * gwei
}

fn input_contains_address(input: &[u8], address: &Address) -> bool {
    if input.len() < 36 {
        return false;
    }
    let addr_bytes = address.as_slice(); // 20 bytes
    // ABI encoding: addresses are padded to 32 bytes with 12 leading zeros
    // Search in 32-byte aligned slots after the 4-byte selector
    let data = &input[4..]; // skip selector
    for chunk in data.chunks(32) {
        if chunk.len() == 32 && chunk[..12] == [0u8; 12] && chunk[12..] == *addr_bytes {
            return true;
        }
    }
    false
}

struct Sniper {
    trader: Arc<Trader>,
    bundle_sender: BundleSender,
    token_memory: TokenMemory,
    tx_seen: Arc<DashSet<B256>>,
    dev_creates: Arc<DashMap<Address, Vec<std::time::Instant>>>, // dev -> creation timestamps
    current_block: Arc<std::sync::atomic::AtomicU64>,
}

impl Sniper {
    async fn new(private_key: &str) -> Result<Self> {
        let trader = Arc::new(Trader::new(private_key).await?);

        // Initialize nonce manager from blockchain
        trader.init_nonce().await?;

        // Initialize with current block number from RPC
        let initial_block = trader.get_block_number().await;

        Ok(Self {
            trader: trader.clone(),
            bundle_sender: BundleSender::new(trader.signer.clone()),
            token_memory: Arc::new(DashMap::new()),
            tx_seen: Arc::new(DashSet::new()),
            dev_creates: Arc::new(DashMap::new()),
            current_block: Arc::new(std::sync::atomic::AtomicU64::new(initial_block)),
        })
    }

    /// Check if dev is rate-limited (too many token creations recently)
    fn is_dev_rate_limited(&self, dev: Address) -> bool {
        let window = std::time::Duration::from_secs(*DEV_RATE_LIMIT_WINDOW_SECS);
        let max_count = *DEV_RATE_LIMIT_COUNT;
        let now = std::time::Instant::now();

        let mut entry = self.dev_creates.entry(dev).or_insert_with(Vec::new);
        // Remove old entries outside the window
        entry.retain(|t| now.duration_since(*t) < window);
        // Record this creation
        entry.push(now);

        if entry.len() > max_count {
            warn!(
                "🚫 Dev {:?} rate-limited: {} tokens in last {}s (max {})",
                dev,
                entry.len(),
                DEV_RATE_LIMIT_WINDOW_SECS.to_string(),
                max_count
            );
            return true;
        }
        false
    }

    fn get_current_block(&self) -> u64 {
        self.current_block
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    fn update_current_block(&self, block: u64) {
        self.current_block
            .store(block, std::sync::atomic::Ordering::Relaxed);
    }

    async fn multithread_buy(&self, token: Address) -> Result<U256> {
        let buy_amount = alloy::primitives::utils::parse_ether(&BUY_AMOUNT_ETH.to_string())?;
        let gas_price = self.trader.get_gas_price().await + 2_000_000_000;

        // Always fetch on-chain nonce for bundle txs
        let nonce = self.trader.get_onchain_nonce().await?;
        info!("Using on-chain nonce {} for multithread buy", nonce);

        let buy_tx = self
            .trader
            .build_buy_tx_with_nonce(token, buy_amount, nonce, gas_price)
            .await?;

        let block = self.trader.get_block_number().await;
        self.bundle_sender.send_bundle(vec![buy_tx], block).await?;

        sleep(Duration::from_secs(2)).await;

        self.trader.get_token_balance(token).await
    }

    async fn multithread_sell(&self, token: Address, amount: U256) -> Result<serde_json::Value> {
        let adjusted_amount = amount - U256::from(*DUST_AMOUNT_WEI);
        let gas_price = self.trader.get_gas_price().await + 2_000_000_000;

        // Always fetch on-chain nonce for bundle txs
        let base_nonce = self.trader.get_onchain_nonce().await?;
        info!("Using on-chain nonce {} for multithread sell (approve+sell)", base_nonce);

        let (approve_result, sell_result) = tokio::join!(
            self.trader
                .build_approve_tx_with_nonce(token, adjusted_amount, base_nonce, gas_price),
            self.trader
                .build_sell_tx_with_nonce(token, adjusted_amount, base_nonce + 1, gas_price)
        );

        let approve_tx = approve_result?;
        let sell_tx = sell_result?;

        let block = self.trader.get_block_number().await;
        let response = self
            .bundle_sender
            .send_bundle(vec![approve_tx, sell_tx], block)
            .await?;

        info!("Sold {} of token {:?}", adjusted_amount, token);

        Ok(serde_json::json!({
            "result": response.result,
            "error": response.error
        }))
    }

    // REMOVED: decode_and_process() - no longer needed
    // We don't care about other buyers, only developer sells

    /// Handle new token creation - backrun with bundle
    /// Bundle: [raw_create_tx, buy_tx]
    async fn handle_token_launch(
        &self,
        tx_hash: B256,
        from_addr: Address,
        input_data: &str,
        value: U256,
        raw_tx: Vec<u8>,
        tx_nonce: u64,
    ) -> Result<()> {
        use chrono::Utc;

        info!("NEW TOKEN LAUNCH detected from dev: {:?}", from_addr);

        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let separator = "━".repeat(60);
        let bnb_value = alloy::primitives::utils::format_ether(value);

        // Decode full token params from calldata
        let input_bytes = hex::decode(input_data.trim_start_matches("0x")).ok();
        let decoded_params = input_bytes
            .as_ref()
            .and_then(|b| decode_create_token_calldata(b));

        // Instantly predict token address from calldata (no API call needed)
        let predicted_token = input_bytes.as_ref().and_then(|b| predict_token_address(b));

        if let Some(ref decoded) = decoded_params {
            if let Some(predicted) = predicted_token {
                info!(
                    "\n[{}]\n{}\n🪙 TOKEN CREATION DETECTED\n{}\n  Transaction Hash: {:?}\n  Developer Address: {:?}\n  Predicted Token: {:?}\n  {}\n  Value Sent: {} BNB\n{}\n",
                    timestamp, separator, separator, tx_hash, from_addr, predicted, decoded.params, bnb_value, separator
                );
            } else {
                info!(
                    "\n[{}]\n{}\n🪙 TOKEN CREATION DETECTED\n{}\n  Transaction Hash: {:?}\n  Developer Address: {:?}\n  {}\n  Value Sent: {} BNB\n{}\n",
                    timestamp, separator, separator, tx_hash, from_addr, decoded.params, bnb_value, separator
                );
            }
        } else {
            info!(
                "\n[{}]\n{}\n🪙 TOKEN CREATION DETECTED\n{}\n  Transaction Hash: {:?}\n  Developer Address: {:?}\n  Function Selector: {}\n  Value Sent: {} BNB\n{}\n",
                timestamp, separator, separator, tx_hash, from_addr, &input_data[..10], bnb_value, separator
            );
        }

        // Use pre-decoded name/symbol if available
        let token_name: Option<String> = decoded_params.as_ref().map(|d| d.params.name.clone());
        let token_symbol: Option<String> = decoded_params.as_ref().map(|d| d.params.symbol.clone());
        let dev_buy_cost: Option<U256> = decoded_params.as_ref().map(|d| d.params.fee2);

        // ---- FILTER 0: Nonce — only buy if dev's tx nonce is low (fresh wallet / early token) ----
        if tx_nonce >= 4 {
            info!(
                "⏭️  Skipping token: dev tx nonce {} >= 4 (likely not an early creator)",
                tx_nonce
            );
            return Ok(());
        }

        // ---- FILTER 1: Min dev buy — skip tokens with insufficient dev commitment ----
        let min_dev_buy_wei = alloy::primitives::utils::parse_ether(&MIN_DEV_BUY_BNB.to_string())?;
        match dev_buy_cost {
            Some(cost) if cost < min_dev_buy_wei => {
                let cost_bnb = alloy::primitives::utils::format_ether(cost);
                info!(
                    "⏭️  Skipping token: dev buy {} BNB < min {} BNB",
                    cost_bnb, *MIN_DEV_BUY_BNB
                );
                return Ok(());
            }
            None => {
                info!("⏭️  Skipping token: no dev buy detected");
                return Ok(());
            }
            _ => {} // dev buy >= min, continue
        }

        // ---- FILTER 2: Dev rate limit ----
        if self.is_dev_rate_limited(from_addr) {
            return Ok(());
        }

        // Get fresh block number directly from RPC (cached value may be stale)
        let current_block = self.trader.get_block_number().await;
        info!("Starting backrun bundle at block {}", current_block);

        let predicted_addr = match predicted_token {
            Some(addr) => {
                info!("✓ Token address predicted: {:?}", addr);
                addr
            }
            None => {
                warn!("✗ Failed to predict token address, skipping backrun");
                return Ok(());
            }
        };

        // Format dev buy info
        let dev_buy_str = match dev_buy_cost {
            Some(cost) => {
                let cost_bnb = alloy::primitives::utils::format_ether(cost);
                format!("Unknown amount of tokens (cost: {} BNB)", cost_bnb)
            }
            _ => "None (no initial buy)".to_string(),
        };

        let name_str = token_name.unwrap_or_else(|| "UNKNOWN".to_string());
        let symbol_str = token_symbol.unwrap_or_else(|| "???".to_string());

        // Log token creation with dev buy info
        info!(
            "\n[{}]\n{}\n🪙 NEW TOKEN CREATED\n{}\n  Token Address: {:?}\n  Name: {} ({})\n  Developer: {:?}\n  Dev Initial Buy: {}\n{}\n",
            timestamp, separator, separator, predicted_addr, name_str, symbol_str, from_addr, dev_buy_str, separator
        );

        // ---- Multi-relay triple-bundle: 9 requests in parallel ----
        // 3 bundles (A, B, C) × 3 relays (48Club, BlockRazor, NodeReal)
        // Bundle A: [createToken, buy] → block N     (ideal backrun)
        // Bundle B: [buy] → block N                   (standalone, same block)
        // Bundle C: [buy] → block N+1                 (standalone, next block)
        // All use same nonce — only ONE can succeed across all relays.
        let buy_amount = alloy::primitives::utils::parse_ether(&BUY_AMOUNT_ETH.to_string())?;
        let gas_price = *DEFAULT_GAS_PRICE;

        let nonce = match self.trader.get_onchain_nonce().await {
            Ok(n) => n,
            Err(e) => {
                error!("Failed to get on-chain nonce: {}", e);
                return Err(e);
            }
        };
        info!("Using on-chain nonce {} for multi-relay triple-bundle", nonce);

        let buy_tx = match self
            .trader
            .build_buy_tx_with_nonce(predicted_addr, buy_amount, nonce, gas_price)
            .await
        {
            Ok(tx) => tx,
            Err(e) => {
                return Err(e);
            }
        };

        info!(
            "\n[{}]\n{}\n📡 DISPATCHING MULTI-RELAY TRIPLE-BUNDLE\n{}\n  Token: {:?}\n  Buy Amount: {} BNB\n  Relays: 48Club, BlockRazor, NodeReal\n  Bundles: A[block {}]=create+buy, B[block {}]=buy, C[block {}]=buy\n  Total requests: 9 (3 bundles × 3 relays)\n{}\n",
            timestamp, separator, separator, predicted_addr, *BUY_AMOUNT_ETH,
            current_block, current_block, current_block + 1, separator
        );

        // Fire all 9 requests in parallel — fire and forget
        self.bundle_sender
            .dispatch_triple_bundle(raw_tx, buy_tx, current_block)
            .await;

        // ---- Populate position with REAL balance immediately ----
        // After dispatching bundles, check on-chain balance.
        // If > 0: set position with real tokens right away.
        // If 0: wait 1s and retry (block might still be mining).
        let buy_amount_for_verify = buy_amount;
        let verify_token = predicted_addr;

        // First immediate check
        let balance = self.trader.get_token_balance(predicted_addr).await.unwrap_or(U256::ZERO);

        if !balance.is_zero() {
            // Buy landed — set real position immediately
            info!("✅ Buy landed immediately: {} tokens of {:?}", balance, predicted_addr);

            // Also fetch dev's initial token balance
            let dev_balance = self.trader
                .get_token_balance_for(predicted_addr, from_addr)
                .await
                .unwrap_or(U256::ZERO);

            self.token_memory.insert(
                verify_token,
                TokenInfo {
                    token: verify_token,
                    developer: from_addr,
                    our_position: balance,
                    cost_basis_bnb: buy_amount_for_verify,
                    peak_value_bnb: buy_amount_for_verify,
                    last_value_bnb: U256::ZERO,
                    last_value_update: std::time::Instant::now(),
                    dev_initial_balance: dev_balance,
                    dev_cumulative_sold: U256::ZERO,
                    created_at: std::time::Instant::now(),
                },
            );
            info!("📊 Dev {:?} holds {} tokens of {:?}", from_addr, dev_balance, verify_token);

            // Send approve immediately with ultra-cheap gas (no rush, just needs to land eventually)
            let trader_appro = self.trader.clone();
            let token_appro = verify_token;
            tokio::spawn(async move {
                match trader_appro
                    .build_approve_tx_with_nonce(
                        token_appro,
                        U256::MAX, // unlimited approve
                        trader_appro.get_onchain_nonce().await.unwrap_or(0),
                        *APPROVE_GAS_PRICE,
                    )
                    .await
                {
                    Ok(raw_tx) => {
                        match trader_appro.send_raw_tx(raw_tx).await {
                            Ok(hash) => info!(
                                "✅ Approve sent for {:?} at 0.05 gwei: {:?}",
                                token_appro, hash
                            ),
                            Err(e) => warn!("Approve send failed for {:?}: {}", token_appro, e),
                        }
                    }
                    Err(e) => warn!("Approve build failed for {:?}: {}", token_appro, e),
                }
            });
        } else {
            // Not landed yet — insert placeholder, wait 1s and retry
            self.token_memory.insert(
                verify_token,
                TokenInfo {
                    token: verify_token,
                    developer: from_addr,
                    our_position: U256::ZERO,
                    cost_basis_bnb: buy_amount_for_verify,
                    peak_value_bnb: U256::ZERO,
                    last_value_bnb: U256::ZERO,
                    last_value_update: std::time::Instant::now(),
                    dev_initial_balance: U256::ZERO,
                    dev_cumulative_sold: U256::ZERO,
                    created_at: std::time::Instant::now(),
                },
            );

            let sniper_verify = self.trader.clone();
            let token_mem = self.token_memory.clone();
            tokio::spawn(async move {
                sleep(Duration::from_secs(*POSITION_VERIFY_DELAY_SECS)).await;
                match sniper_verify.get_token_balance(verify_token).await {
                    Ok(balance) if balance.is_zero() => {
                        warn!(
                            "⚠️ Position verification: 0 balance for {:?} — backrun likely failed, removing",
                            verify_token
                        );
                        token_mem.remove(&verify_token);
                    }
                    Ok(balance) => {
                        info!(
                            "✅ Position verified after delay: {} tokens of {:?}",
                            balance, verify_token
                        );
                        let dev_balance = sniper_verify
                            .get_token_balance_for(verify_token, from_addr)
                            .await
                            .unwrap_or(U256::ZERO);
                        if let Some(mut entry) = token_mem.get_mut(&verify_token) {
                            entry.our_position = balance;
                            entry.peak_value_bnb = buy_amount_for_verify;
                            entry.dev_initial_balance = dev_balance;
                        }
                        info!(
                            "📊 Dev {:?} holds {} tokens of {:?}",
                            from_addr, dev_balance, verify_token
                        );

                        // Send approve with ultra-cheap gas
                        let trader_appro = sniper_verify.clone();
                        let token_appro = verify_token;
                        tokio::spawn(async move {
                            match trader_appro
                                .build_approve_tx_with_nonce(
                                    token_appro,
                                    U256::MAX,
                                    trader_appro.get_onchain_nonce().await.unwrap_or(0),
                                    *APPROVE_GAS_PRICE,
                                )
                                .await
                            {
                                Ok(raw_tx) => {
                                    match trader_appro.send_raw_tx(raw_tx).await {
                                        Ok(hash) => info!(
                                            "✅ Approve sent for {:?} at 0.05 gwei: {:?}",
                                            token_appro, hash
                                        ),
                                        Err(e) => warn!("Approve send failed for {:?}: {}", token_appro, e),
                                    }
                                }
                                Err(e) => warn!("Approve build failed for {:?}: {}", token_appro, e),
                            }
                        });
                    }
                    Err(e) => {
                        warn!(
                            "Position verification RPC error for {:?}: {}",
                            verify_token, e
                        );
                    }
                }
            });
        }

        Ok(())
    }

    /// Trigger frontrun sell when developer sells ANY amount of tokens
    /// Bundle: [our_approve_tx, our_sell_tx, dev_sell_tx]
    async fn handle_potential_dev_sell(
        &self,
        _tx_hash: B256,
        from_addr: Address,
        input_bytes: &[u8],
        raw_tx: Vec<u8>,
    ) -> Result<()> {
        use chrono::Utc;

        // Extract token and amount from calldata
        let (token, sell_amount) = match extract_token_from_sell(input_bytes) {
            Some(params) => params,
            None => {
                warn!("Failed to decode sell parameters from calldata");
                return Ok(());
            }
        };

        // Check if this token is one we're tracking and the seller is the developer
        let info = match self.token_memory.get(&token) {
            Some(info) if info.developer == from_addr => info.clone(),
            _ => return Ok(()), // Not a tracked token or not the developer
        };

        // Check if we have a position to sell
        if info.our_position.is_zero() {
            info!("Dev sell detected but we have no position in {:?}", token);
            return Ok(());
        }

        // ---- Calculate dev sell percentages (single tx + cumulative) ----
        // dev_balance_before = current_balance + sell_amount (balance hasn't changed yet, tx is pending)
        let dev_balance = match self.trader.get_token_balance_for(token, from_addr).await {
            Ok(bal) => bal,
            Err(e) => {
                error!("Failed to get dev token balance: {}", e);
                return Ok(());
            }
        };
        let dev_balance_before = dev_balance + sell_amount;

        // Lazy-init dev_initial_balance if not yet set
        let dev_initial_balance = if info.dev_initial_balance.is_zero() {
            // First sell we see — dev_balance_before IS the initial balance
            if let Some(mut entry) = self.token_memory.get_mut(&token) {
                entry.dev_initial_balance = dev_balance_before;
            }
            dev_balance_before
        } else {
            info.dev_initial_balance
        };

        // Single-tx sell %
        let dev_sell_pct = if dev_balance_before.is_zero() {
            100.0
        } else {
            let pct_u256 = sell_amount * U256::from(10000) / dev_balance_before;
            pct_u256.to::<u64>() as f64 / 100.0
        };

        // Cumulative sell % (including this pending tx)
        let cumulative_sold = info.dev_cumulative_sold + sell_amount;
        let cumulative_pct = if dev_initial_balance.is_zero() {
            100.0
        } else {
            let pct_u256 = cumulative_sold * U256::from(10000) / dev_initial_balance;
            pct_u256.to::<u64>() as f64 / 100.0
        };

        info!(
            "📊 Dev sell analysis: this tx {:.1}%, cumulative {:.1}% (total sold {} / initial {})",
            dev_sell_pct, cumulative_pct, cumulative_sold, dev_initial_balance
        );

        // ---- Apply sell strategy ----
        // Key insight: even if THIS sell is small (e.g. 4%), cumulative might be huge
        // If cumulative >= CUMULATIVE_DUMP_PCT → dump everything regardless of single-tx size
        let force_dump = cumulative_pct >= *DEV_SELL_CUMULATIVE_DUMP_PCT;

        if !force_dump && dev_sell_pct < *DEV_SELL_IGNORE_PCT {
            info!(
                "⏭️  Ignoring dev sell: {:.1}% < {:.1}% threshold (cumulative {:.1}% < {:.1}%) for token {:?}",
                dev_sell_pct, *DEV_SELL_IGNORE_PCT, cumulative_pct, *DEV_SELL_CUMULATIVE_DUMP_PCT, token
            );
            // Still track the cumulative even when ignoring
            if let Some(mut entry) = self.token_memory.get_mut(&token) {
                entry.dev_cumulative_sold = cumulative_sold;
            }
            return Ok(());
        }

        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let separator = "━".repeat(60);

        warn!(
            "🚨 DEV SELL DETECTED! Dev {:?} selling {:.1}% ({}) of token {:?}",
            from_addr, dev_sell_pct, sell_amount, token
        );

        // Build frontrun bundle: [approve_tx, sell_tx, dev_sell_tx]
        let current_block = self.trader.get_block_number().await;
        let gas_price = self.trader.get_gas_price().await + *FRONTRUN_GAS_PREMIUM;

        // Get our token balance
        let our_balance = match self.trader.get_token_balance(token).await {
            Ok(bal) => bal,
            Err(e) => {
                error!("Failed to get token balance: {}", e);
                return Ok(());
            }
        };

        if our_balance.is_zero() {
            info!("No balance to sell for token {:?}", token);
            return Ok(());
        }

        // Determine how much to sell based on dev sell %
        let our_sell_amount = if force_dump || dev_sell_pct >= *DEV_SELL_DUMP_PCT {
            // Dev dumping (single large sell OR cumulative drip exceeded threshold)
            if force_dump {
                info!(
                    "💀 Cumulative dev sells {:.1}% >= {:.1}% → dumping entire position",
                    cumulative_pct, *DEV_SELL_CUMULATIVE_DUMP_PCT
                );
            } else {
                info!(
                    "💀 Dev dumping {:.1}% >= {:.1}% → selling entire position",
                    dev_sell_pct, *DEV_SELL_DUMP_PCT
                );
            }
            our_balance
        } else {
            // Proportional — sell same % as dev
            let proportional =
                our_balance * U256::from((dev_sell_pct * 100.0) as u64) / U256::from(10000u64);
            info!(
                "📉 Dev selling {:.1}% → we sell proportional: {} tokens",
                dev_sell_pct, proportional
            );
            proportional
        };

        let sell_amount_adjusted = align_to_gwei(our_sell_amount.saturating_sub(U256::from(*DUST_AMOUNT_WEI)));

        if sell_amount_adjusted.is_zero() {
            info!("Sell amount too small after dust/gwei adjustment, skipping");
            return Ok(());
        }

        // Always fetch on-chain nonce for bundle txs
        let sell_nonce = match self.trader.get_onchain_nonce().await {
            Ok(n) => n,
            Err(e) => {
                error!("Failed to get on-chain nonce for frontrun: {}", e);
                return Err(e);
            }
        };
        info!("Using on-chain nonce {} for frontrun sell", sell_nonce);

        // Build only sell tx — approve was sent post-buy with cheap gas
        let sell_tx = match self
            .trader
            .build_sell_tx_with_nonce(token, sell_amount_adjusted, sell_nonce, gas_price)
            .await
        {
            Ok(tx) => tx,
            Err(e) => {
                error!("Failed to build sell tx for frontrun: {}", e);
                return Err(e);
            }
        };

        // Send frontrun bundle via Puissant
        // Dispatch to ALL relays x 2 blocks — Bundle A: [sell, dev_tx], Bundle B: [sell], Bundle C: [sell]
        info!(
            "\n[{}]\n{}\n🚨 FRONTRUN DISPATCHING\n{}\n  Token: {:?}\n  Our Sell: {} tokens ({:.1}% of position)\n  Dev Sell: {} tokens ({:.1}%)\n  Relays: 48Club, BlockRazor, NodeReal\n  Bundles: A[block N]=[sell+dev], B[block N]=[sell], C[block N+1]=[sell]\n{}\n",
            timestamp, separator, separator, token, sell_amount_adjusted, dev_sell_pct, sell_amount, dev_sell_pct, separator
        );
        self.bundle_sender
            .dispatch_frontrun(sell_tx, raw_tx, current_block)
            .await;

        // Update position tracking (fire-and-forget dispatch)
        if force_dump || dev_sell_pct >= *DEV_SELL_DUMP_PCT {
            self.token_memory.remove(&token);
        } else {
            if let Some(mut entry) = self.token_memory.get_mut(&token) {
                let sold = sell_amount_adjusted.min(entry.our_position);
                if !entry.our_position.is_zero() {
                    let remaining = entry.our_position.saturating_sub(sold);
                    entry.cost_basis_bnb = entry.cost_basis_bnb * remaining / entry.our_position;
                    entry.our_position = remaining;
                }
                entry.dev_cumulative_sold = cumulative_sold;
            }
        }

        Ok(())
    }

    /// Handle dev sell via proxy contract (not directly to four.meme)
    /// We detected token address in the input data of a tx from a tracked dev
    /// Frontrun bundle: [our_approve_tx, our_sell_tx, dev_proxy_sell_tx]
    async fn handle_dev_proxy_sell(
        &self,
        tx_hash: B256,
        dev_addr: Address,
        token: Address,
        raw_tx: Vec<u8>,
    ) -> Result<()> {
        use chrono::Utc;

        let info = match self.token_memory.get(&token) {
            Some(info) if info.developer == dev_addr => info.clone(),
            _ => return Ok(()),
        };

        if info.our_position.is_zero() {
            info!("Dev proxy sell detected but we have no position in {:?}", token);
            return Ok(());
        }

        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let separator = "━".repeat(60);

        // Get our token balance
        let our_balance = match self.trader.get_token_balance(token).await {
            Ok(bal) => bal,
            Err(e) => {
                error!("Failed to get our token balance: {}", e);
                return Ok(());
            }
        };

        if our_balance.is_zero() {
            info!("No balance to sell for token {:?}", token);
            self.token_memory.remove(&token);
            return Ok(());
        }

        warn!(
            "\n[{}]\n{}\n🚨 DEV PROXY SELL DETECTED\n{}\n  Dev: {:?}\n  Token: {:?}\n  Tx Hash: {:?}\n  Our Balance: {} tokens\n  Action: DUMP ENTIRE POSITION (proxy sell = suspicious)\n{}\n",
            timestamp, separator, separator, dev_addr, token, tx_hash, our_balance, separator
        );

        // For proxy sells, always dump entire position — if dev is using a proxy,
        // they're trying to hide their sell, which is very bearish
        let sell_amount = align_to_gwei(our_balance.saturating_sub(U256::from(*DUST_AMOUNT_WEI)));
        if sell_amount.is_zero() {
            info!("Sell amount too small after dust/gwei adjustment");
            return Ok(());
        }

        let current_block = self.trader.get_block_number().await;
        let gas_price = self.trader.get_gas_price().await + *FRONTRUN_GAS_PREMIUM;

        let sell_nonce = match self.trader.get_onchain_nonce().await {
            Ok(n) => n,
            Err(e) => {
                error!("Failed to get on-chain nonce for proxy frontrun: {}", e);
                return Err(e);
            }
        };
        info!("Using on-chain nonce {} for proxy frontrun sell", sell_nonce);

        // Build only sell tx — approve was sent post-buy
        let sell_tx = match self
            .trader
            .build_sell_tx_with_nonce(token, sell_amount, sell_nonce, gas_price)
            .await
        {
            Ok(tx) => tx,
            Err(e) => return Err(e),
        };

        // Frontrun: dispatch to ALL relays — Bundle A: [sell, dev_tx], B: [sell], C: [sell]
        self.bundle_sender
            .dispatch_frontrun(sell_tx, raw_tx, current_block)
            .await;

        // Remove from tracking (proxy sell = dump entire position)
        self.token_memory.remove(&token);

        Ok(())
    }

    /// Emergency sell with higher gas — dispatch to ALL relays x 3 blocks
    async fn emergency_sell(&self, token: Address, amount: U256) -> Result<serde_json::Value> {
        info!("EMERGENCY SELL: {} of token {:?}", amount, token);

        let adjusted_amount = align_to_gwei(amount.saturating_sub(U256::from(*DUST_AMOUNT_WEI)));
        let gas_price = self.trader.get_gas_price().await + *FRONTRUN_GAS_PREMIUM;

        // Fetch on-chain nonce
        let sell_nonce = self.trader.get_onchain_nonce().await?;
        info!("Using on-chain nonce {} for emergency sell", sell_nonce);

        // Build only sell tx — approve was already sent post-buy
        let sell_tx = match self
            .trader
            .build_sell_tx_with_nonce(token, adjusted_amount, sell_nonce, gas_price)
            .await
        {
            Ok(tx) => tx,
            Err(e) => return Err(e),
        };

        let block = self.trader.get_block_number().await;

        // Dispatch to ALL relays x 2 blocks (N, N+1) — 6 requests total
        info!(
            "📡 Dispatching emergency sell to all relays x 2 blocks ({}-{})",
            block, block + 1
        );
        self.bundle_sender
            .dispatch_triple_sell(vec![], sell_tx, block)
            .await;

        // Remove from tracking
        self.token_memory.remove(&token);

        info!("Emergency sell dispatched: {} of token {:?}", adjusted_amount, token);

        Ok(serde_json::json!({
            "result": "dispatched to all relays",
            "error": null
        }))
    }
}

fn setup_logging() {
    use std::fs;

    let log_dir = "logs";
    fs::create_dir_all(log_dir).ok();

    // Create 4 separate log files
    let app_log = RollingFileAppender::new(Rotation::DAILY, log_dir, "app.log");
    let detected_tokens_log =
        RollingFileAppender::new(Rotation::DAILY, log_dir, "detected_tokens.log");
    let unknown_txs_log = RollingFileAppender::new(Rotation::DAILY, log_dir, "unknown_txs.log");
    let trades_log = RollingFileAppender::new(Rotation::DAILY, log_dir, "trades.log");

    let (app_nb, _guard1) = tracing_appender::non_blocking(app_log);
    let (_tokens_nb, _guard2) = tracing_appender::non_blocking(detected_tokens_log);
    let (_unknown_nb, _guard3) = tracing_appender::non_blocking(unknown_txs_log);
    let (_trades_nb, _guard4) = tracing_appender::non_blocking(trades_log);

    // For now, write all logs to all files (simplified approach)
    // Can be enhanced later with target-based routing
    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stdout).with_ansi(true))
        .with(fmt::layer().with_writer(app_nb).with_ansi(false))
        .with(tracing_subscriber::filter::LevelFilter::from_level(
            Level::INFO,
        ))
        .init();

    // Store guard references to prevent drop (keep files open)
    std::mem::forget(_guard1);
    std::mem::forget(_guard2);
    std::mem::forget(_guard3);
    std::mem::forget(_guard4);
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    setup_logging();

    let private_key = std::env::var("PRIVATE_KEY").expect("PRIVATE_KEY must be set");

    let sniper = Arc::new(Sniper::new(&private_key).await?);

    info!("Sniper initialized, address: {:?}", sniper.trader.address());
    info!("Target contract: {:?}", *MEME_CONTRACT_ADDRESS);
    info!("WebSocket endpoint: {}", &*BSC_WS);

    // Spawn block number tracker
    let sniper_blocks = Arc::clone(&sniper);
    tokio::spawn(async move {
        loop {
            if let Err(e) = track_blocks(sniper_blocks.clone()).await {
                warn!("Block tracker error: {}, reconnecting in 5s...", e);
                sleep(Duration::from_secs(5)).await;
            }
        }
    });

    // Spawn periodic cleanup task (memory leak prevention)
    let sniper_cleanup = Arc::clone(&sniper);
    tokio::spawn(async move {
        loop {
            sleep(Duration::from_secs(300)).await; // every 5 minutes
                                                   // Clean tx_seen (unbounded growth)
            let tx_count_before = sniper_cleanup.tx_seen.len();
            if tx_count_before > 50_000 {
                sniper_cleanup.tx_seen.clear();
                info!("🧹 Cleared tx_seen: {} entries removed", tx_count_before);
            }
            // Clean dev_creates — remove devs with no recent activity
            let dev_window = std::time::Duration::from_secs(*DEV_RATE_LIMIT_WINDOW_SECS);
            let now = std::time::Instant::now();
            sniper_cleanup.dev_creates.retain(|_, timestamps| {
                timestamps.retain(|t| now.duration_since(*t) < dev_window);
                !timestamps.is_empty()
            });
            // Log memory stats
            info!(
                "📊 Memory: tx_seen={}, dev_creates={}, token_memory={}",
                sniper_cleanup.tx_seen.len(),
                sniper_cleanup.dev_creates.len(),
                sniper_cleanup.token_memory.len(),
            );
        }
    });

    // Spawn take-profit + trailing stop-loss + stagnation monitor.
    // Uses HelperManager.trySell for exact BNB output (after fees), no approximations.
    // - Take-profit: sell when current value >= cost * (1 + TAKE_PROFIT_PCT/100)
    // - Trailing stop-loss: sell when current value < peak * (1 - STOP_LOSS_PCT/100)
    //   Peak is updated every check if current value > previous peak.
    // - Stagnation: sell if value hasn't changed for STAGNATION_TTL_SECS (dead token).
    let has_tp = *TAKE_PROFIT_PCT > 0.0;
    let has_sl = *STOP_LOSS_PCT > 0.0;
    let has_stagnation = *STAGNATION_TTL_SECS > 0;
    if has_tp || has_sl || has_stagnation {
        let sniper_monitor = Arc::clone(&sniper);
        let tp_target = *TAKE_PROFIT_PCT;
        let sl_target = *STOP_LOSS_PCT;
        let stagnation_ttl_secs = *STAGNATION_TTL_SECS;
        tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(*PROFIT_CHECK_INTERVAL_SECS)).await;

                // Collect positions that have a balance and cost basis
                let positions: Vec<Address> = sniper_monitor
                    .token_memory
                    .iter()
                    .filter(|entry| !entry.our_position.is_zero() && !entry.cost_basis_bnb.is_zero())
                    .map(|entry| entry.token)
                    .collect();

                for token in positions {
                    // We need to re-read the entry inside the loop to get mutable access
                    let info = match sniper_monitor.token_memory.get(&token) {
                        Some(info) => info.clone(),
                        None => continue,
                    };

                    // trySell returns exact BNB output after fees — no approximation
                    match sniper_monitor.trader.try_sell(token, info.our_position).await {
                        Ok(bnb_out) => {
                            // ---- Update trailing stop-loss peak ----
                            if has_sl {
                                let peak = info.peak_value_bnb;
                                let mut new_peak = peak;
                                if bnb_out > peak {
                                    // Price went up, update peak
                                    new_peak = bnb_out;
                                    if let Some(mut entry) = sniper_monitor.token_memory.get_mut(&token) {
                                        entry.peak_value_bnb = new_peak;
                                    }
                                    info!(
                                        "📈 Trailing SL peak updated for {:?}: {} BNB → {} BNB",
                                        token,
                                        alloy::primitives::utils::format_ether(peak),
                                        alloy::primitives::utils::format_ether(new_peak)
                                    );
                                }

                                // Check if we hit stop-loss: bnb_out < peak * (1 - SL/100)
                                let sl_threshold = new_peak
                                    * U256::from(10000u64 - (sl_target * 100.0) as u64)
                                    / U256::from(10000);
                                if bnb_out < sl_threshold {
                                    let drop_pct = if new_peak.is_zero() {
                                        0.0
                                    } else {
                                        let drop = new_peak - bnb_out;
                                        (drop * U256::from(10000) / new_peak).to::<u64>() as f64 / 100.0
                                    };
                                    warn!(
                                        "🛑 TRAILING STOP-LOSS HIT! {:?} peak: {} BNB, current: {} BNB (drop {:.1}% >= {:.1}%) → dumping",
                                        token,
                                        alloy::primitives::utils::format_ether(new_peak),
                                        alloy::primitives::utils::format_ether(bnb_out),
                                        drop_pct, sl_target
                                    );
                                    if let Err(e) = sniper_monitor.emergency_sell(token, info.our_position).await {
                                        error!("Stop-loss sell failed for {:?}: {}", token, e);
                                    }
                                    continue; // Position was sold, skip take-profit check
                                }
                            }

                            // ---- Check take-profit ----
                            if has_tp {
                                let cost = info.cost_basis_bnb;
                                if bnb_out > cost {
                                    let diff = bnb_out - cost;
                                    let profit_pct =
                                        (diff * U256::from(10000) / cost).to::<u64>() as f64 / 100.0;

                                    if profit_pct >= tp_target {
                                        warn!(
                                            "🎯 TAKE-PROFIT HIT! {:?} profit: {:.1}% >= {:.1}% | cost: {} BNB, output: {} BNB → dumping",
                                            token, profit_pct, tp_target,
                                            alloy::primitives::utils::format_ether(cost),
                                            alloy::primitives::utils::format_ether(bnb_out)
                                        );
                                        if let Err(e) = sniper_monitor.emergency_sell(token, info.our_position).await {
                                            error!("Take-profit sell failed for {:?}: {}", token, e);
                                        }
                                        continue; // Sold, go to next token
                                    }
                                }
                            }

                            // ---- Check price stagnation (dead token detection) ----
                            if has_stagnation {
                                if let Some(mut entry) = sniper_monitor.token_memory.get_mut(&token) {
                                    if bnb_out != entry.last_value_bnb {
                                        // Price moved (up or down) — token is alive, reset timer
                                        entry.last_value_bnb = bnb_out;
                                        entry.last_value_update = std::time::Instant::now();
                                    } else if entry.last_value_update.elapsed().as_secs() >= stagnation_ttl_secs {
                                        // Price stagnant — dump immediately
                                        warn!(
                                            "💤 STAGNANT PRICE for {:?} ({} BNB for > {}s) → dumping",
                                            token,
                                            alloy::primitives::utils::format_ether(bnb_out),
                                            stagnation_ttl_secs
                                        );
                                        if let Err(e) = sniper_monitor.emergency_sell(token, info.our_position).await {
                                            error!("Stagnation sell failed for {:?}: {}", token, e);
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Monitor trySell error for {:?}: {}", token, e);
                        }
                    }
                }
            }
        });
    }

    // Spawn dev balance monitor — polls dev token balance to catch sells we miss from mempool
    let sniper_monitor = Arc::clone(&sniper);
    tokio::spawn(async move {
        loop {
            sleep(Duration::from_millis(1000)).await;

            // Collect all tracked positions with known dev balances
            let positions: Vec<(Address, Address, U256)> = sniper_monitor
                .token_memory
                .iter()
                .filter(|entry| !entry.dev_initial_balance.is_zero())
                .map(|entry| (entry.token, entry.developer, entry.dev_initial_balance))
                .collect();

            for (token, dev, last_known_balance) in positions {
                match sniper_monitor.trader.get_token_balance_for(token, dev).await {
                    Ok(current_balance) if current_balance < last_known_balance => {
                        let sold = last_known_balance - current_balance;
                        let pct = if last_known_balance.is_zero() {
                            100.0
                        } else {
                            let p = sold * U256::from(10000) / last_known_balance;
                            p.to::<u64>() as f64 / 100.0
                        };
                        warn!(
                            "🔍 POLL: Dev {:?} balance dropped! {} → {} (sold {} = {:.1}%) for token {:?}",
                            dev, last_known_balance, current_balance, sold, pct, token
                        );

                        // Get our balance and sell everything
                        let our_balance = sniper_monitor
                            .trader
                            .get_token_balance(token)
                            .await
                            .unwrap_or(U256::ZERO);

                        if our_balance.is_zero() {
                            warn!("No balance to sell for {:?}, removing", token);
                            sniper_monitor.token_memory.remove(&token);
                            continue;
                        }

                        warn!(
                            "🚨 EMERGENCY SELL triggered by poll — dumping {} tokens of {:?}",
                            our_balance, token
                        );
                        match sniper_monitor.emergency_sell(token, our_balance).await {
                            Ok(resp) => info!("Poll-triggered sell result: {}", resp),
                            Err(e) => error!("Poll-triggered sell failed: {}", e),
                        }
                    }
                    Ok(current_balance) => {
                        // Update stored balance in case dev bought more
                        if current_balance != last_known_balance {
                            if let Some(mut entry) = sniper_monitor.token_memory.get_mut(&token) {
                                entry.dev_initial_balance = current_balance;
                            }
                        }
                    }
                    Err(_) => {} // RPC error, skip
                }
            }
        }
    });

    loop {
        if let Err(e) = run_subscription(sniper.clone()).await {
            error!("Subscription error: {}, reconnecting in 5s...", e);
            sleep(Duration::from_secs(5)).await;
        }
    }
}

/// Track current block number via RPC polling instead of a separate gRPC stream.
/// The relay only allows 1 stream per API key — we need that slot for NewTxs.
async fn track_blocks(sniper: Arc<Sniper>) -> Result<()> {
    info!("Starting block tracker via RPC polling");

    let mut last_logged = 0u64;
    loop {
        let block = sniper.trader.get_block_number().await;
        if block > 0 {
            let prev = sniper.get_current_block();
            if block != prev {
                sniper.update_current_block(block);
                // Only log every 10 blocks to reduce spam
                if prev > 0 && block >= last_logged + 10 {
                    info!("Block: {} (+{})", block, block - last_logged);
                    last_logged = block;
                }
                if last_logged == 0 {
                    last_logged = block;
                }
            }
        }
        // BSC produces blocks every ~0.3-0.5s, poll frequently
        sleep(Duration::from_millis(200)).await;
    }
}

/// Subscribe to pending transactions via WebSocket (eth_subscribe newPendingTransactions)
/// For each pending tx hash, fetch full tx via RPC and process it.
async fn run_subscription_ws(sniper: Arc<Sniper>) -> Result<()> {
    info!("Connecting to WebSocket: {}", &*BSC_WS);

    let ws = WsConnect::new(&*BSC_WS);
    let ws_provider = ProviderBuilder::new().connect_ws(ws).await?;

    info!("WebSocket connected, subscribing to pending transactions...");

    let sub = ws_provider.subscribe_full_pending_transactions().await?;
    let mut stream = sub.into_stream();

    info!("Subscribed via WS to newPendingTransactions (full tx mode)");

    let target = *MEME_CONTRACT_ADDRESS;

    while let Some(tx) = stream.next().await {
        // Filter by target contract OR by tracked dev address
        let to_addr = match tx.to() {
            Some(addr) => addr,
            None => continue, // Skip contract creation txs
        };

        let from_addr = tx.from();

        // Check if this tx is from a tracked developer (for proxy sell detection)
        let is_tracked_dev = sniper.token_memory.iter().any(|entry| entry.developer == from_addr);

        if to_addr != target && !is_tracked_dev {
            continue;
        }

        let tx_hash = tx.tx_hash();

        // Deduplication check
        if !sniper.tx_seen.insert(tx_hash) {
            continue;
        }

        let input_data = tx.input().to_vec();
        let value = tx.value();

        if input_data.len() < 4 {
            continue;
        }

        // Check if this is a dev tx to an external contract (proxy sell)
        let is_proxy_sell = is_tracked_dev && to_addr != target;

        if is_proxy_sell {
            // Dev is sending tx to some other contract — check if input contains tracked token address
            let matched_token = sniper.token_memory.iter().find(|entry| {
                entry.developer == from_addr && input_contains_address(&input_data, entry.key())
            });

            if let Some(entry) = matched_token {
                let token = *entry.key();
                let _info = entry.clone();
                drop(entry); // Release DashMap ref

                warn!(
                    "🔍 Dev {:?} proxy sell detected! to={:?} selector=0x{} token={:?}",
                    from_addr, to_addr, hex::encode(&input_data[..4]), token
                );

                // Encode raw tx for bundle inclusion
                use alloy::eips::eip2718::Encodable2718;
                let mut raw_tx_buf = Vec::new();
                tx.inner.encode_2718(&mut raw_tx_buf);

                let sniper_clone = sniper.clone();
                let raw_tx_clone = raw_tx_buf;
                tokio::spawn(async move {
                    if let Err(e) = sniper_clone
                        .handle_dev_proxy_sell(tx_hash, from_addr, token, raw_tx_clone)
                        .await
                    {
                        error!("Proxy sell handler error: {}", e);
                    }
                });
            }
            continue;
        }

        let input_hex = format!("0x{}", hex::encode(&input_data));
        let tx_type = categorize_tx(&input_hex);

        // Only log CreateToken or tracked dev sells
        let should_log = match tx_type {
            TxType::CreateToken => true,
            TxType::SellToken => {
                if let Some((token, _)) = extract_token_from_sell(&input_data) {
                    sniper
                        .token_memory
                        .get(&token)
                        .map(|info| info.developer == from_addr)
                        .unwrap_or(false)
                } else {
                    false
                }
            }
            TxType::BuyToken | TxType::Unknown => false,
        };

        if should_log {
            info!("Pending tx detected, hash: {:?}", tx_hash);
        }

        // Log transaction details
        match tx_type {
            TxType::SellToken => {
                if let Some((token, amount)) = extract_token_from_sell(&input_data) {
                    if let Some(info) = sniper.token_memory.get(&token) {
                        if info.developer == from_addr {
                            info!(
                                "TX RECEIVED | hash: {:?} | from: {:?} (DEV) | to: {:?} | token: {:?} | amount: {} | selector: {}",
                                tx_hash, from_addr, to_addr, token, amount, &input_hex[..10]
                            );
                        }
                    }
                }
            }
            TxType::CreateToken => {
                info!(
                    "TX RECEIVED | hash: {:?} | from: {:?} | to: {:?} | value: {} BNB | selector: {}",
                    tx_hash, from_addr, to_addr,
                    alloy::primitives::utils::format_ether(value),
                    if input_hex.len() >= 10 { &input_hex[..10] } else { &input_hex }
                );
            }
            TxType::BuyToken | TxType::Unknown => {}
        }

        if tx_type == TxType::Unknown {
            continue;
        }

        // Encode raw tx for bundle inclusion
        use alloy::eips::eip2718::Encodable2718;
        let mut raw_tx_buf = Vec::new();
        tx.inner.encode_2718(&mut raw_tx_buf);

        let sniper_clone = sniper.clone();
        let input_data_clone = input_data.clone();
        let input_hex_clone = input_hex.clone();
        let raw_tx_clone = raw_tx_buf;
        let tx_nonce = tx.nonce();

        tokio::spawn(async move {
            let result = match tx_type {
                TxType::CreateToken => {
                    sniper_clone
                        .handle_token_launch(
                            tx_hash,
                            from_addr,
                            &input_hex_clone,
                            value,
                            raw_tx_clone,
                            tx_nonce,
                        )
                        .await
                }
                TxType::BuyToken => Ok(()),
                TxType::SellToken => {
                    sniper_clone
                        .handle_potential_dev_sell(
                            tx_hash,
                            from_addr,
                            &input_data_clone,
                            raw_tx_clone,
                        )
                        .await
                }
                TxType::Unknown => Ok(()),
            };

            if let Err(e) = result {
                error!(
                    "PROCESSING ERROR | type: {:?} | hash: {:?} | from: {:?} | error: {}",
                    tx_type, tx_hash, from_addr, e
                );
            }
        });
    }

    warn!("WebSocket stream ended, will reconnect...");
    Ok(())
}

/// Subscribe to pending transactions via IPC — lower latency than WS.
/// IPC avoids TCP handshake and WebSocket framing overhead.
async fn run_subscription_ipc(sniper: Arc<Sniper>) -> Result<()> {
    let ipc_path = BSC_IPC.as_ref().expect("IPC path must be set");
    info!("Connecting to IPC: {}", ipc_path);

    let ipc = IpcConnect::new(ipc_path.clone());
    let provider = ProviderBuilder::new().connect_ipc(ipc).await?;

    info!("IPC connected, subscribing to pending transactions...");

    let sub = provider.subscribe_full_pending_transactions().await?;
    let mut stream = sub.into_stream();

    info!("Subscribed via IPC to newPendingTransactions (full tx mode)");

    let target = *MEME_CONTRACT_ADDRESS;

    while let Some(tx) = stream.next().await {
        // Filter by target contract OR by tracked dev address
        let to_addr = match tx.to() {
            Some(addr) => addr,
            None => continue, // Skip contract creation txs
        };

        let from_addr = tx.from();

        // Check if this tx is from a tracked developer (for proxy sell detection)
        let is_tracked_dev = sniper.token_memory.iter().any(|entry| entry.developer == from_addr);

        if to_addr != target && !is_tracked_dev {
            continue;
        }

        let tx_hash = tx.tx_hash();

        // Deduplication check
        if !sniper.tx_seen.insert(tx_hash) {
            continue;
        }

        let input_data = tx.input().to_vec();
        let value = tx.value();

        if input_data.len() < 4 {
            continue;
        }

        // Check if this is a dev tx to an external contract (proxy sell)
        let is_proxy_sell = is_tracked_dev && to_addr != target;

        if is_proxy_sell {
            // Dev is sending tx to some other contract — check if input contains tracked token address
            let matched_token = sniper.token_memory.iter().find(|entry| {
                entry.developer == from_addr && input_contains_address(&input_data, entry.key())
            });

            if let Some(entry) = matched_token {
                let token = *entry.key();
                let _info = entry.clone();
                drop(entry); // Release DashMap ref

                warn!(
                    "🔍 Dev {:?} proxy sell detected! to={:?} selector=0x{} token={:?}",
                    from_addr, to_addr, hex::encode(&input_data[..4]), token
                );

                // Encode raw tx for bundle inclusion
                use alloy::eips::eip2718::Encodable2718;
                let mut raw_tx_buf = Vec::new();
                tx.inner.encode_2718(&mut raw_tx_buf);

                let sniper_clone = sniper.clone();
                let raw_tx_clone = raw_tx_buf;
                tokio::spawn(async move {
                    if let Err(e) = sniper_clone
                        .handle_dev_proxy_sell(tx_hash, from_addr, token, raw_tx_clone)
                        .await
                    {
                        error!("Proxy sell handler error: {}", e);
                    }
                });
            }
            continue;
        }

        let input_hex = format!("0x{}", hex::encode(&input_data));
        let tx_type = categorize_tx(&input_hex);

        // Only log CreateToken or tracked dev sells
        let should_log = match tx_type {
            TxType::CreateToken => true,
            TxType::SellToken => {
                if let Some((token, _)) = extract_token_from_sell(&input_data) {
                    sniper
                        .token_memory
                        .get(&token)
                        .map(|info| info.developer == from_addr)
                        .unwrap_or(false)
                } else {
                    false
                }
            }
            TxType::BuyToken | TxType::Unknown => false,
        };

        if should_log {
            info!("Pending tx detected, hash: {:?}", tx_hash);
        }

        // Log transaction details
        match tx_type {
            TxType::SellToken => {
                if let Some((token, amount)) = extract_token_from_sell(&input_data) {
                    if let Some(info) = sniper.token_memory.get(&token) {
                        if info.developer == from_addr {
                            info!(
                                "TX RECEIVED | hash: {:?} | from: {:?} (DEV) | to: {:?} | token: {:?} | amount: {} | selector: {}",
                                tx_hash, from_addr, to_addr, token, amount, &input_hex[..10]
                            );
                        }
                    }
                }
            }
            TxType::CreateToken => {
                info!(
                    "TX RECEIVED | hash: {:?} | from: {:?} | to: {:?} | value: {} BNB | selector: {}",
                    tx_hash, from_addr, to_addr,
                    alloy::primitives::utils::format_ether(value),
                    if input_hex.len() >= 10 { &input_hex[..10] } else { &input_hex }
                );
            }
            TxType::BuyToken | TxType::Unknown => {}
        }

        if tx_type == TxType::Unknown {
            continue;
        }

        // Encode raw tx for bundle inclusion
        use alloy::eips::eip2718::Encodable2718;
        let mut raw_tx_buf = Vec::new();
        tx.inner.encode_2718(&mut raw_tx_buf);

        let sniper_clone = sniper.clone();
        let input_data_clone = input_data.clone();
        let input_hex_clone = input_hex.clone();
        let raw_tx_clone = raw_tx_buf;
        let tx_nonce = tx.nonce();

        tokio::spawn(async move {
            let result = match tx_type {
                TxType::CreateToken => {
                    sniper_clone
                        .handle_token_launch(
                            tx_hash,
                            from_addr,
                            &input_hex_clone,
                            value,
                            raw_tx_clone,
                            tx_nonce,
                        )
                        .await
                }
                TxType::BuyToken => Ok(()),
                TxType::SellToken => {
                    sniper_clone
                        .handle_potential_dev_sell(
                            tx_hash,
                            from_addr,
                            &input_data_clone,
                            raw_tx_clone,
                        )
                        .await
                }
                TxType::Unknown => Ok(()),
            };

            if let Err(e) = result {
                error!(
                    "PROCESSING ERROR | type: {:?} | hash: {:?} | from: {:?} | error: {}",
                    tx_type, tx_hash, from_addr, e
                );
            }
        });
    }

    warn!("IPC stream ended, will reconnect...");
    Ok(())
}

/// Subscribe to pending transactions via IPC (preferred) or WebSocket fallback.
/// IPC has lower latency (~0.1ms) vs WS (~1-5ms) because it avoids TCP/handshake overhead.
/// IPC is used if `BSC_IPC` env var is set AND the socket file exists.
async fn run_subscription(sniper: Arc<Sniper>) -> Result<()> {
    // Determine transport: IPC > WS (IPC is faster, WS is fallback)
    if let Some(ref ipc_path) = *BSC_IPC {
        if std::path::Path::new(ipc_path).exists() {
            match run_subscription_ipc(sniper.clone()).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    warn!(
                        "IPC subscription failed ({}), falling back to WS",
                        e
                    );
                }
            }
        } else {
            warn!(
                "BSC_IPC set to '{}' but socket file not found, falling back to WS",
                ipc_path
            );
        }
    }

    // WebSocket fallback
    run_subscription_ws(sniper).await
}
