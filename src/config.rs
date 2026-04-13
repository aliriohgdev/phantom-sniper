use alloy::primitives::Address;
use std::env;
use std::str::FromStr;
use std::sync::LazyLock;

// ============== RPC / WebSocket / IPC ==============

pub static BSC_RPC: LazyLock<String> =
    LazyLock::new(|| env::var("BSC_RPC").unwrap_or_else(|_| "http://127.0.0.1:8545".to_string()));

pub static BSC_WS: LazyLock<String> =
    LazyLock::new(|| env::var("BSC_WS").unwrap_or_else(|_| "ws://127.0.0.1:8546".to_string()));

/// Optional IPC path for lower-latency subscriptions.
/// If set and the IPC socket exists, it is used instead of WebSocket.
/// Typical geth default: `/home/user/.ethereum/geth.ipc`
pub static BSC_IPC: LazyLock<Option<String>> =
    LazyLock::new(|| env::var("BSC_IPC").ok().or_else(|| Some("/opt/bsc-data/geth.ipc".to_string())));

// ============== MEV Relays ==============

/// 48Club Puissant — ~25% BSC hashrate
pub static RELAY_48CLUB: LazyLock<String> = LazyLock::new(|| {
    env::var("RELAY_48CLUB").unwrap_or_else(|_| "https://puissant-bsc.48.club".to_string())
});

/// BlockRazor Builder — ~37% BSC hashrate
pub static RELAY_BLOCKRAZOR: LazyLock<String> = LazyLock::new(|| {
    env::var("RELAY_BLOCKRAZOR")
        .unwrap_or_else(|_| "https://frankfurt.builder.blockrazor.io".to_string())
});

/// BlockRazor auth token (required header)
pub static BLOCKRAZOR_AUTH_TOKEN: LazyLock<Option<String>> =
    LazyLock::new(|| env::var("BLOCKRAZOR_AUTH_TOKEN").ok());

/// NodeReal MEV — ~5% BSC hashrate
pub static RELAY_NODEREAL: LazyLock<String> = LazyLock::new(|| {
    env::var("RELAY_NODEREAL")
        .unwrap_or_else(|_| "https://bsc-mainnet.nodereal.io/mev/v1/5db9047f23724133b9714f274061db0b".to_string())
});

// ============== Chain constants ==============

pub const BSC_CHAIN_ID: u64 = 56;

/// Default gas price in wei (used for approve/sell when not frontrunning).
pub static DEFAULT_GAS_PRICE: LazyLock<u128> = LazyLock::new(|| {
    env::var("DEFAULT_GAS_PRICE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1_500_000_000) // 1.5 gwei
});

/// Gas limit for buy transactions.
pub static BUY_GAS_LIMIT: LazyLock<u64> = LazyLock::new(|| {
    env::var("BUY_GAS_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300_000)
});

/// Gas limit for approve transactions.
pub static APPROVE_GAS_LIMIT: LazyLock<u64> = LazyLock::new(|| {
    env::var("APPROVE_GAS_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000)
});

/// Gas limit for sell transactions.
pub static SELL_GAS_LIMIT: LazyLock<u64> = LazyLock::new(|| {
    env::var("SELL_GAS_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(500_000)
});

pub static BUY_AMOUNT_ETH: LazyLock<f64> = LazyLock::new(|| {
    env::var("BUY_AMOUNT_BNB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.001)
});

/// Small dust amount subtracted from sell amounts to avoid rounding issues (wei).
pub static DUST_AMOUNT_WEI: LazyLock<u128> = LazyLock::new(|| {
    env::var("DUST_AMOUNT_WEI")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1_800_000_000_000) // 0.0000018 BNB
});

/// four.meme TokenManager contract address on BSC.
pub static MEME_CONTRACT_ADDRESS: LazyLock<Address> = LazyLock::new(|| {
    env::var("MEME_CONTRACT_ADDRESS")
        .ok()
        .and_then(|v| Address::from_str(&v).ok())
        .unwrap_or_else(|| {
            Address::from_str("0x5c952063c7fc8610FFDB798152D69F0B9550762b").unwrap()
        })
});

/// HelperManager contract — used for accurate sell simulation (trySell).
/// Returns exact BNB output before executing a real sell.
pub static HELPER_MANAGER_ADDRESS: LazyLock<Address> = LazyLock::new(|| {
    Address::from_str("0xF251F83e40a78868FcfA3FA4599Dad6494E46034").unwrap()
});

/// Max block delta for bundle validity (bundle valid for next N blocks).
pub static MAX_BLOCK_DELTA: LazyLock<u64> = LazyLock::new(|| {
    env::var("MAX_BLOCK_DELTA")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(25) // BSC blocks every ~0.3s, 25 blocks ≈ 8s window
});

/// Max timestamp delta for bundle validity (seconds).
pub static MAX_TIMESTAMP_DELTA: LazyLock<u64> = LazyLock::new(|| {
    env::var("MAX_TIMESTAMP_DELTA")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100)
});

/// Extra gas price premium (wei) for frontrun priority over dev sells.
pub static FRONTRUN_GAS_PREMIUM: LazyLock<u128> = LazyLock::new(|| {
    env::var("FRONTRUN_GAS_PREMIUM")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5_000_000_000) // 5 gwei
});

/// Gas price for post-buy approve tx (wei).
/// Since we send approve immediately after buy (no rush), we can use minimum gas.
pub static APPROVE_GAS_PRICE: LazyLock<u128> = LazyLock::new(|| {
    env::var("APPROVE_GAS_PRICE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50_000_000) // 0.05 gwei — ultra barato
});

// ============== Filters ==============

/// Comma-separated list of developer addresses to blacklist.
/// Tokens created by these addresses are always skipped.
/// Currently disabled — only nonce and min_dev_buy filters are active.
pub static _DEV_BLACKLIST: LazyLock<Vec<Address>> = LazyLock::new(|| {
    env::var("DEV_BLACKLIST")
        .ok()
        .map(|s| {
            s.split(',')
                .filter_map(|addr| addr.trim().parse().ok())
                .collect()
        })
        .unwrap_or_default()
});

/// Minimum dev initial buy (BNB). Tokens with dev buy below this are skipped as spam.
pub static MIN_DEV_BUY_BNB: LazyLock<f64> = LazyLock::new(|| {
    env::var("MIN_DEV_BUY_BNB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.13)
});

/// Max tokens a single dev can create within the rate limit window before being blocked.
pub static DEV_RATE_LIMIT_COUNT: LazyLock<usize> = LazyLock::new(|| {
    env::var("DEV_RATE_LIMIT_COUNT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3)
});

/// Rate limit window in seconds.
pub static DEV_RATE_LIMIT_WINDOW_SECS: LazyLock<u64> = LazyLock::new(|| {
    env::var("DEV_RATE_LIMIT_WINDOW_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3600)
});

// ============== Sell strategy ==============

/// Dev sell below this % of their balance → ignore (gas not worth it).
pub static DEV_SELL_IGNORE_PCT: LazyLock<f64> = LazyLock::new(|| {
    env::var("DEV_SELL_IGNORE_PCT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5.0)
});

/// Dev sell above this % → dump our entire position.
/// Between IGNORE and DUMP → sell proportional %.
pub static DEV_SELL_DUMP_PCT: LazyLock<f64> = LazyLock::new(|| {
    env::var("DEV_SELL_DUMP_PCT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50.0)
});

/// Cumulative dev sells exceed this % of initial balance → dump entire position.
/// Protects against drip-selling (many small sells below IGNORE threshold).
pub static DEV_SELL_CUMULATIVE_DUMP_PCT: LazyLock<f64> = LazyLock::new(|| {
    env::var("DEV_SELL_CUMULATIVE_DUMP_PCT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30.0)
});

// ============== Position management ==============

/// Seconds to wait after backrun before verifying position balance if buy didn't land immediately.
/// Default: 1s (BSC blocks every ~0.3s, 1s ≈ 3 blocks).
pub static POSITION_VERIFY_DELAY_SECS: LazyLock<u64> = LazyLock::new(|| {
    env::var("POSITION_VERIFY_DELAY_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
});

/// Max seconds to hold a position before auto-selling if price stagnates (default: 120 = 2 min).
/// If price changes (up or down), the timer resets.
/// If price stays flat for this long, we sell to avoid dead tokens.
pub static STAGNATION_TTL_SECS: LazyLock<u64> = LazyLock::new(|| {
    env::var("STAGNATION_TTL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(120)
});

/// Take-profit target as percentage of cost basis.
/// 50 = sell when position value reaches 150% of our BNB cost (50% profit).
/// 0 = disabled (use TTL only).
pub static TAKE_PROFIT_PCT: LazyLock<f64> = LazyLock::new(|| {
    env::var("TAKE_PROFIT_PCT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0)
});

/// Trailing stop-loss percentage.
/// 30 = sell if current value drops 30% from the peak (highest value seen).
/// The stop-loss trails upward as the position gains value.
/// 0 = disabled.
pub static STOP_LOSS_PCT: LazyLock<f64> = LazyLock::new(|| {
    env::var("STOP_LOSS_PCT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0)
});

/// How often to check profit targets and stop-loss (seconds).
pub static PROFIT_CHECK_INTERVAL_SECS: LazyLock<u64> = LazyLock::new(|| {
    env::var("PROFIT_CHECK_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
});

// ============== Function selectors for four.meme ==============

pub const CREATE_TOKEN_SELECTOR: &str = "0x519ebb10"; // createToken(bytes,bytes)
pub const BUY_TOKEN_SELECTOR_1: &str = "0x87f27655"; // buyTokenAMAP(address,uint256,uint256)
pub const BUY_TOKEN_SELECTOR_2: &str = "0x7f79f6df"; // buyTokenAMAP(address,address,uint256,uint256)
pub const BUY_TOKEN_SELECTOR_3: &str = "0xedf9e251"; // buy with USD1 stablecoin (uint256,address,uint256,uint256)
pub const SELL_TOKEN_SELECTOR_1: &str = "0x3e11741f"; // sellToken(address,uint256,uint256)
pub const SELL_TOKEN_SELECTOR_2: &str = "0xf464e7db"; // sellToken(address,uint256)
pub const SELL_TOKEN_SELECTOR_3: &str = "0x06e7b98f"; // sellToken(uint256,address,uint256,uint256,uint256,address)
pub const SELL_TOKEN_SELECTOR_4: &str = "0x0da74935"; // sellToken(uint256,address,uint256,uint256)
