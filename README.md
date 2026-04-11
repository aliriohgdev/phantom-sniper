# BSC Token Sniper Bot

Rust-based sniper bot for BSC (BNB Smart Chain) targeting [four.meme](https://four.meme) token launches.

## How It Works

1. **Monitors mempool** via IPC (preferred) or WebSocket (`eth_subscribe newPendingTransactions`) on own BSC node
2. **Detects new token creation** (`createToken`) on the four.meme contract
3. **Backruns** the creation tx — buys the token in the same block via MEV bundle
4. **Tracks developer sells** — if dev sells, frontrun with our sell to exit before price drops
5. **Bundles submitted** to [48Club Puissant](https://puissant-bsc.48.club) (free BSC MEV relay)

## Architecture

```
src/
├── main.rs       # Entry point: WebSocket subscription, tx categorization, handlers
├── config.rs     # Constants, env vars, contract addresses, function selectors
├── trader.rs     # Tx building/signing, nonce management, gas/block caching
├── bundle.rs     # MEV bundle submission to Puissant (eth_sendBundle)
├── contracts.rs  # Solidity ABI definitions (IERC20, ITokenManager) via alloy::sol!
└── decoder.rs    # Decode createToken calldata, predict token address via CREATE2
```

## Requirements

- **Own BSC node** with IPC socket or WebSocket + txpool enabled (geth/erigon)
- **Rust nightly** (1.89+)
- **Funded wallet** on BSC mainnet

## Setup

### 1. BSC Node

The bot connects to your own BSC node via **IPC** (preferred, lower latency) or **WebSocket** (fallback).

**IPC (recommended):**
- Node must have IPC enabled (geth enables by default)
- Set `BSC_IPC` in `.env` to the path of the IPC socket file
- Typical paths: `~/.ethereum/geth.ipc` (Linux), `~/Library/Ethereum/geth.ipc` (macOS)

**WebSocket (fallback):**
- WebSocket enabled (`WSHost = "127.0.0.1"`, `WSPort = 8546`)
- `eth`, `net`, `web3`, `txpool` modules in `WSModules`
- Full sync complete

### 2. Configure

```bash
cp .env.example .env
# Edit .env — set PRIVATE_KEY
```

### 3. Build & Run

```bash
cargo build --release
./target/release/sniper
```

## Environment Variables

All configuration is via `.env` file. Copy `.env.example` to `.env` and adjust values.

### Connection

| Variable | Default | Description |
|----------|---------|-------------|
| `PRIVATE_KEY` | — | Wallet private key (without 0x prefix) |
| `BSC_RPC` | `http://127.0.0.1:8545` | BSC node HTTP RPC |
| `BSC_WS` | `ws://127.0.0.1:8546` | BSC node WebSocket |
| `BSC_IPC` | _(unset)_ | BSC node IPC socket path (if set and exists, used instead of WS) |
| `PUISSANT_RPC` | `https://puissant-bsc.48.club` | MEV bundle relay |

### Trading

| Variable | Default | Description |
|----------|---------|-------------|
| `BUY_AMOUNT_BNB` | `0.001` | BNB amount per token buy |
| `MEME_CONTRACT_ADDRESS` | `0x5c95...762b` | four.meme TokenManager contract |

### Gas

| Variable | Default | Description |
|----------|---------|-------------|
| `BUY_GAS_LIMIT` | `300000` | Gas limit for buy tx |
| `APPROVE_GAS_LIMIT` | `100000` | Gas limit for approve tx |
| `SELL_GAS_LIMIT` | `500000` | Gas limit for sell tx |
| `DEFAULT_GAS_PRICE` | `3000000000` | Default gas price in wei (3 gwei) |
| `FRONTRUN_GAS_PREMIUM` | `5000000000` | Extra gas premium in wei (5 gwei) for frontrun priority |
| `DUST_AMOUNT_WEI` | `1800000000000` | Dust subtracted from sell amounts to avoid rounding |

### MEV Bundle

| Variable | Default | Description |
|----------|---------|-------------|
| `MAX_BLOCK_DELTA` | `3` | Bundle valid for next N blocks |
| `MAX_TIMESTAMP_DELTA` | `100` | Max timestamp delta for bundle validity (seconds) |

### Filters

| Variable | Default | Description |
|----------|---------|-------------|
| `MIN_DEV_BUY_BNB` | `0.05` | Min dev initial buy to consider token (BNB). Below = spam |
| `DEV_BLACKLIST` | _(empty)_ | Comma-separated developer addresses to always skip |
| `DEV_RATE_LIMIT_COUNT` | `3` | Max token creations per dev within window before blocking |
| `DEV_RATE_LIMIT_WINDOW_SECS` | `3600` | Rate limit window (seconds) |

### Sell Strategy

| Variable | Default | Description |
|----------|---------|-------------|
| `DEV_SELL_IGNORE_PCT` | `5` | Dev sell below this % → ignore (gas not worth it) |
| `DEV_SELL_DUMP_PCT` | `50` | Dev sell above this % → dump entire position |
| `DEV_SELL_CUMULATIVE_DUMP_PCT` | `30` | Cumulative dev sells above this % → dump (anti-drip) |

### Position Management

| Variable | Default | Description |
|----------|---------|-------------|
| `POSITION_VERIFY_DELAY_SECS` | `9` | Seconds to wait after backrun to verify token balance |
| `POSITION_TTL_SECS` | `1800` | Max hold time before auto-sell (seconds, 0 = disabled) |
| `TAKE_PROFIT_PCT` | `0` | Sell when profit hits this % above cost (50 = 50% profit, 0 = disabled) |
| `STOP_LOSS_PCT` | `0` | Trailing stop-loss: sell if value drops this % below peak (30 = 30% drop, 0 = disabled) |
| `PROFIT_CHECK_INTERVAL_SECS` | `1` | How often to check profit targets and stop-loss (seconds) |

## Bundle Strategy

### Backrun (token launch)
```
Block N: [createToken_tx, our_buy_tx]
```
Bot predicts token address from calldata via CREATE2, builds buy tx immediately.

### Frontrun (dev sells)
```
Block N: [our_approve_tx, our_sell_tx, dev_sell_tx]
```
Bot detects dev's sell tx in mempool, frontrun with our sell to exit at higher price.

## Development

```bash
cargo check          # Fast compile check
cargo build          # Debug build
cargo clippy         # Lint
cargo fmt            # Format code
```
