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
    LazyLock::new(|| env::var("BSC_IPC").unwrap_or_else(|_| "/opt/bsc-data/geth.ipc".to_string()));


pub static PUISSANT_RPC: LazyLock<String> = LazyLock::new(|| {
    env::var("PUISSANT_RPC").unwrap_or_else(|_| "https://puissant-builder.48.club".to_string())
});

// ============== Chain constants ==============

pub const BSC_CHAIN_ID: u64 = 56;

/// Default gas price in wei (used for approve/sell when not frontrunning).
pub static DEFAULT_GAS_PRICE: LazyLock<u128> = LazyLock::new(|| {
    env::var("DEFAULT_GAS_PRICE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3_000_000_000) // 3 gwei
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

// ============== Filters ==============

/// Minimum dev initial buy (BNB). Tokens with dev buy below this are skipped as spam.
pub static MIN_DEV_BUY_BNB: LazyLock<f64> = LazyLock::new(|| {
    env::var("MIN_DEV_BUY_BNB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.05)
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

/// Seconds to wait after backrun before verifying position balance (default: 9s ≈ 3 BSC blocks).
pub static POSITION_VERIFY_DELAY_SECS: LazyLock<u64> = LazyLock::new(|| {
    env::var("POSITION_VERIFY_DELAY_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(9)
});

/// Max seconds to hold a position before auto-selling (default: 1800 = 30 min).
/// If dev hasn't sold within this time, we sell automatically to free capital.
pub static POSITION_TTL_SECS: LazyLock<u64> = LazyLock::new(|| {
    env::var("POSITION_TTL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1800)
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
