use alloy::{
    consensus::{SignableTransaction, TxLegacy},
    network::{EthereumWallet, TxSigner},
    primitives::{keccak256, Address, B256, TxKind, U256},
    providers::{Provider, ProviderBuilder},
    signers::local::PrivateKeySigner,
};
use eyre::Result;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use tracing::{info, warn};

use crate::config::*;
use crate::contracts::{IHelperManager, ITokenManager, IERC20};

#[derive(Debug, Clone)]
pub struct TokenInfoData {
    pub last_price: U256,
    pub total_supply: U256,
    pub offers: U256,
    pub funds: U256,
    pub status: U256,
}

pub struct CachedValue<T: Clone> {
    value: RwLock<T>,
    last_update: RwLock<Instant>,
    ttl: Duration,
}

impl<T: Clone + Default> CachedValue<T> {
    pub fn new(ttl: Duration) -> Self {
        Self {
            value: RwLock::new(T::default()),
            last_update: RwLock::new(Instant::now() - ttl),
            ttl,
        }
    }

    pub async fn get_or_update<F, Fut>(&self, update_fn: F) -> T
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let last = *self.last_update.read().await;
        if last.elapsed() > self.ttl {
            if let Ok(new_val) = update_fn().await {
                *self.value.write().await = new_val.clone();
                *self.last_update.write().await = Instant::now();
                return new_val;
            }
        }
        self.value.read().await.clone()
    }
}

/// NonceManager with local tracking - avoids race conditions in concurrent bundle building.
///
/// Principle:
/// - Nonce stored locally in AtomicU64
/// - `reserve()` atomically increments without RPC
/// - Sync with blockchain only: init + on error
pub struct NonceManager {
    pub(crate) next_nonce: AtomicU64,
    sync_lock: Mutex<()>,
    initialized: AtomicBool,
}

impl NonceManager {
    pub fn new() -> Self {
        Self {
            next_nonce: AtomicU64::new(0),
            sync_lock: Mutex::new(()),
            initialized: AtomicBool::new(false),
        }
    }

    /// Atomically reserve next nonce (LOCAL TRACKING)
    /// No RPC calls - works with local counter
    pub fn reserve(&self) -> u64 {
        self.next_nonce.fetch_add(1, Ordering::SeqCst)
    }

    /// Reserve N consecutive nonces, returns first
    pub fn reserve_batch(&self, count: u64) -> u64 {
        self.next_nonce.fetch_add(count, Ordering::SeqCst)
    }

    /// Get current nonce WITHOUT increment (read-only)
    pub fn current(&self) -> u64 {
        self.next_nonce.load(Ordering::SeqCst)
    }

    /// Rollback N reserved nonces on tx build failure.
    /// Only rolls back if current nonce equals expected_next (no other thread advanced it).
    pub fn rollback(&self, count: u64) {
        let _ = self
            .next_nonce
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                if current >= count {
                    Some(current - count)
                } else {
                    None
                }
            });
        tracing::warn!("NonceManager rolled back {} nonce(s)", count);
    }

    /// Initial sync with blockchain (call once at startup)
    pub async fn init<P: Provider>(&self, provider: &P, address: Address) -> Result<()> {
        let _guard = self.sync_lock.lock().await;
        let chain_nonce = provider.get_transaction_count(address).await?;
        self.next_nonce.store(chain_nonce, Ordering::SeqCst);
        self.initialized.store(true, Ordering::SeqCst);
        tracing::info!("NonceManager initialized with nonce: {}", chain_nonce);
        Ok(())
    }

    /// Force sync (call on "nonce too low" error)
    pub async fn force_sync<P: Provider>(&self, provider: &P, address: Address) -> Result<()> {
        let _guard = self.sync_lock.lock().await;
        let chain_nonce = provider.get_transaction_count(address).await?;
        let local = self.next_nonce.load(Ordering::SeqCst);
        if chain_nonce > local {
            self.next_nonce.store(chain_nonce, Ordering::SeqCst);
            tracing::warn!("NonceManager force synced: {} -> {}", local, chain_nonce);
        }
        Ok(())
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::SeqCst)
    }
}

impl Default for NonceManager {
    fn default() -> Self {
        Self::new()
    }
}

pub type HttpProvider = alloy::providers::fillers::FillProvider<
    alloy::providers::fillers::JoinFill<
        alloy::providers::fillers::JoinFill<
            alloy::providers::Identity,
            alloy::providers::fillers::JoinFill<
                alloy::providers::fillers::GasFiller,
                alloy::providers::fillers::JoinFill<
                    alloy::providers::fillers::BlobGasFiller,
                    alloy::providers::fillers::JoinFill<
                        alloy::providers::fillers::NonceFiller,
                        alloy::providers::fillers::ChainIdFiller,
                    >,
                >,
            >,
        >,
        alloy::providers::fillers::WalletFiller<EthereumWallet>,
    >,
    alloy::providers::RootProvider,
>;

pub struct Trader {
    pub provider: Arc<HttpProvider>,
    pub signer: PrivateKeySigner,
    pub nonce_manager: NonceManager,
    gas_price_cache: CachedValue<u128>,
    block_cache: CachedValue<u64>,
}

impl Trader {
    pub async fn new(private_key: &str) -> Result<Self> {
        let signer: PrivateKeySigner = private_key.parse()?;
        let wallet = EthereumWallet::from(signer.clone());
        let provider = ProviderBuilder::new()
            .wallet(wallet)
            .connect_http(BSC_RPC.as_str().parse()?);

        Ok(Self {
            provider: Arc::new(provider),
            signer,
            nonce_manager: NonceManager::new(),
            gas_price_cache: CachedValue::new(Duration::from_secs(3)),
            block_cache: CachedValue::new(Duration::from_millis(200)),
        })
    }

    pub fn address(&self) -> Address {
        self.signer.address()
    }

    pub async fn get_gas_price(&self) -> u128 {
        let provider = self.provider.clone();
        self.gas_price_cache
            .get_or_update(|| async move {
                let price = provider.get_gas_price().await?;
                Ok(price)
            })
            .await
    }

    pub async fn get_block_number(&self) -> u64 {
        let provider = self.provider.clone();
        self.block_cache
            .get_or_update(|| async move {
                let block = provider.get_block_number().await?;
                Ok(block)
            })
            .await
    }

    pub async fn get_balance(&self) -> Result<U256> {
        Ok(self.provider.get_balance(self.address()).await?)
    }

    pub async fn get_token_balance(&self, token: Address) -> Result<U256> {
        let contract = IERC20::new(token, self.provider.clone());
        let result = contract.balanceOf(self.address()).call().await?;
        Ok(result)
    }

    pub async fn get_token_balance_for(&self, token: Address, holder: Address) -> Result<U256> {
        let contract = IERC20::new(token, self.provider.clone());
        let result = contract.balanceOf(holder).call().await?;
        Ok(result)
    }

    pub async fn get_token_info(&self, token: Address) -> Result<TokenInfoData> {
        let contract = ITokenManager::new(*MEME_CONTRACT_ADDRESS, self.provider.clone());
        let result = contract._tokenInfos(token).call().await?;
        Ok(TokenInfoData {
            last_price: result.lastPrice,
            total_supply: result.totalSupply,
            offers: result.offers,
            funds: result.funds,
            status: result.status,
        })
    }

    /// Simulate selling `amount` tokens via HelperManager.trySell.
    /// Returns the exact BNB output (`funds`) after fees — does NOT execute a tx.
    /// Used for accurate profit calculation before deciding to sell.
    pub async fn try_sell(&self, token: Address, amount: U256) -> Result<U256> {
        let contract = IHelperManager::new(*HELPER_MANAGER_ADDRESS, self.provider.clone());
        let result = contract.trySell(token, amount).call().await?;
        Ok(result.funds)
    }

    /// Send a raw signed tx to the mempool (not via MEV bundle).
    /// Used for post-buy approve with cheap gas.
    pub async fn send_raw_tx(&self, raw_tx: Vec<u8>) -> Result<B256> {
        // Send via eth_sendRawTransaction
        let tx_hex = format!("0x{}", hex::encode(&raw_tx));
        let hash = self.provider.raw_request::<_, B256>(
            std::borrow::Cow::Borrowed("eth_sendRawTransaction"),
            vec![tx_hex],
        ).await?;
        info!("✅ Raw tx sent: {:?}", hash);
        Ok(hash)
    }

    async fn sign_legacy_tx(&self, tx: TxLegacy) -> Result<Vec<u8>> {
        let mut tx = tx;
        let sig = self.signer.sign_transaction(&mut tx).await?;
        let signed = tx.into_signed(sig);

        use alloy::eips::eip2718::Encodable2718;
        let mut buf = Vec::new();
        signed.encode_2718(&mut buf);
        Ok(buf)
    }

    /// Build buy tx - reserves nonce atomically
    /// Used for simple buy operations (non-bundle)
    pub async fn build_buy_tx(&self, token: Address, amount_wei: U256) -> Result<Vec<u8>> {
        let contract = ITokenManager::new(*MEME_CONTRACT_ADDRESS, self.provider.clone());
        let call = contract.buyTokenAMAP_0(token, amount_wei, U256::ZERO);
        let input = call.calldata().clone();

        // Reserve nonce atomically
        let nonce = self.nonce_manager.reserve();
        let gas_price = self.get_gas_price().await;

        let tx = TxLegacy {
            chain_id: Some(BSC_CHAIN_ID),
            nonce,
            gas_price,
            gas_limit: *BUY_GAS_LIMIT,
            to: TxKind::Call(*MEME_CONTRACT_ADDRESS),
            value: amount_wei,
            input: input.into(),
        };

        self.sign_legacy_tx(tx).await
    }

    /// Build approve tx - reserves nonce atomically
    pub async fn build_approve_tx(&self, token: Address, amount: U256) -> Result<Vec<u8>> {
        let contract = IERC20::new(token, self.provider.clone());
        let call = contract.approve(*MEME_CONTRACT_ADDRESS, amount);
        let input = call.calldata().clone();

        // Reserve nonce atomically
        let nonce = self.nonce_manager.reserve();

        let tx = TxLegacy {
            chain_id: Some(BSC_CHAIN_ID),
            nonce,
            gas_price: *DEFAULT_GAS_PRICE,
            gas_limit: *APPROVE_GAS_LIMIT,
            to: TxKind::Call(token),
            value: U256::ZERO,
            input: input.into(),
        };

        self.sign_legacy_tx(tx).await
    }

    /// Build sell tx - reserves nonce atomically
    pub async fn build_sell_tx(&self, token: Address, amount: U256) -> Result<Vec<u8>> {
        let contract = ITokenManager::new(*MEME_CONTRACT_ADDRESS, self.provider.clone());
        let call = contract.sellToken_0(token, amount, U256::ZERO);
        let input = call.calldata().clone();

        // Reserve nonce atomically
        let nonce = self.nonce_manager.reserve();

        let tx = TxLegacy {
            chain_id: Some(BSC_CHAIN_ID),
            nonce,
            gas_price: *DEFAULT_GAS_PRICE,
            gas_limit: *SELL_GAS_LIMIT,
            to: TxKind::Call(*MEME_CONTRACT_ADDRESS),
            value: U256::ZERO,
            input: input.into(),
        };

        self.sign_legacy_tx(tx).await
    }

    /// Build sell tx with custom gas - reserves nonce atomically
    pub async fn build_sell_tx_with_gas(
        &self,
        token: Address,
        amount: U256,
        gas_price: u128,
    ) -> Result<Vec<u8>> {
        let contract = ITokenManager::new(*MEME_CONTRACT_ADDRESS, self.provider.clone());
        let call = contract.sellToken_0(token, amount, U256::ZERO);
        let input = call.calldata().clone();

        // Reserve nonce atomically
        let nonce = self.nonce_manager.reserve();

        let tx = TxLegacy {
            chain_id: Some(BSC_CHAIN_ID),
            nonce,
            gas_price,
            gas_limit: *SELL_GAS_LIMIT,
            to: TxKind::Call(*MEME_CONTRACT_ADDRESS),
            value: U256::ZERO,
            input: input.into(),
        };

        self.sign_legacy_tx(tx).await
    }

    /// Build buy tx with explicit nonce for backrun bundles
    pub async fn build_buy_tx_with_nonce(
        &self,
        token: Address,
        amount_wei: U256,
        nonce: u64,
        gas_price: u128,
    ) -> Result<Vec<u8>> {
        let contract = ITokenManager::new(*MEME_CONTRACT_ADDRESS, self.provider.clone());
        let call = contract.buyTokenAMAP_0(token, amount_wei, U256::ZERO);
        let input = call.calldata().clone();

        let tx = TxLegacy {
            chain_id: Some(BSC_CHAIN_ID),
            nonce,
            gas_price,
            gas_limit: *BUY_GAS_LIMIT,
            to: TxKind::Call(*MEME_CONTRACT_ADDRESS),
            value: amount_wei,
            input: input.into(),
        };

        self.sign_legacy_tx(tx).await
    }

    /// Build approve tx with explicit nonce for frontrun bundles
    pub async fn build_approve_tx_with_nonce(
        &self,
        token: Address,
        amount: U256,
        nonce: u64,
        gas_price: u128,
    ) -> Result<Vec<u8>> {
        let contract = IERC20::new(token, self.provider.clone());
        let call = contract.approve(*MEME_CONTRACT_ADDRESS, amount);
        let input = call.calldata().clone();

        let tx = TxLegacy {
            chain_id: Some(BSC_CHAIN_ID),
            nonce,
            gas_price,
            gas_limit: *APPROVE_GAS_LIMIT,
            to: TxKind::Call(token),
            value: U256::ZERO,
            input: input.into(),
        };

        self.sign_legacy_tx(tx).await
    }

    /// Build sell tx with explicit nonce for frontrun bundles
    pub async fn build_sell_tx_with_nonce(
        &self,
        token: Address,
        amount: U256,
        nonce: u64,
        gas_price: u128,
    ) -> Result<Vec<u8>> {
        let contract = ITokenManager::new(*MEME_CONTRACT_ADDRESS, self.provider.clone());
        let call = contract.sellToken_0(token, amount, U256::ZERO);
        let input = call.calldata().clone();

        let tx = TxLegacy {
            chain_id: Some(BSC_CHAIN_ID),
            nonce,
            gas_price,
            gas_limit: *SELL_GAS_LIMIT,
            to: TxKind::Call(*MEME_CONTRACT_ADDRESS),
            value: U256::ZERO,
            input: input.into(),
        };

        self.sign_legacy_tx(tx).await
    }

    /// Initialize nonce manager from blockchain (call once at startup)
    pub async fn init_nonce(&self) -> Result<()> {
        self.nonce_manager
            .init(&*self.provider, self.address())
            .await
    }

    /// Get on-chain nonce directly from RPC.
    /// Use this for MEV bundle txs — bundles are speculative and may not get mined,
    /// so we must always use the current on-chain nonce (not an incrementing local counter).
    pub async fn get_onchain_nonce(&self) -> Result<u64> {
        let nonce = self.provider.get_transaction_count(self.address()).await?;
        // Also keep local NonceManager in sync
        self.nonce_manager.next_nonce.store(nonce, std::sync::atomic::Ordering::SeqCst);
        Ok(nonce)
    }

    /// Atomically reserve a nonce for bundle tx
    pub fn reserve_nonce(&self) -> u64 {
        self.nonce_manager.reserve()
    }

    /// Atomically reserve N consecutive nonces for bundle txs
    pub fn reserve_nonces(&self, count: u64) -> u64 {
        self.nonce_manager.reserve_batch(count)
    }

    /// Rollback 1 reserved nonce on tx build failure
    pub fn rollback_nonce(&self) {
        self.nonce_manager.rollback(1);
    }

    /// Rollback N reserved nonces on tx build failure
    pub fn rollback_nonces(&self, count: u64) {
        self.nonce_manager.rollback(count);
    }

    /// Force sync nonce with blockchain (call on "nonce too low" error)
    pub async fn force_sync_nonce(&self) -> Result<()> {
        self.nonce_manager
            .force_sync(&*self.provider, self.address())
            .await
    }
}
