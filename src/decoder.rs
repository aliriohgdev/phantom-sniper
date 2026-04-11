use alloy::primitives::{keccak256, Address, B256, U256};
use std::sync::LazyLock;
use tracing::warn;

/// Cached init code hash — computed once, reused for every predict_token_address call.
static CACHED_INIT_CODE_HASH: LazyLock<B256> = LazyLock::new(compute_init_code_hash);

/// Init code hash for four.meme token contract (constant for all tokens)
/// Computed from: keccak256(init_code)
pub const TOKEN_INIT_CODE_HASH: [u8; 32] = {
    // Init code from CREATE2 trace - this is the token bytecode
    // We compute keccak256 at compile time is not possible, so we'll compute it once at runtime
    // For now, placeholder - will be set after first computation
    [0u8; 32]
};

/// Compute keccak256 hash of the token init code
pub fn compute_init_code_hash() -> B256 {
    // Token init code from four.meme factory CREATE2 opcode
    let init_code = hex::decode(
        "608060405234801561000f575f80fd5b506100193361001e565b61006f565b600580546001600160a01b038381166001600160a01b0319831681179093556040519116919082907f8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e0905f90a35050565b610eee8061007c5f395ff3fe608060405234801561000f575f80fd5b5060043610610127575f3560e01c806370a08231116100a9578063a9059cbb1161006e578063a9059cbb14610245578063c5c03af314610258578063d72dd3b414610261578063dd62ed3e14610274578063f2fde38b14610287575f80fd5b806370a08231146101df578063715018a6146102075780638da5cb5b1461020f57806395d89b411461022a578063a457c2d714610232575f80fd5b80632eabc917116100ef5780632eabc91714610199578063313ce567146101ae57806332be6330146101bd57806339509351146101c55780633af3d783146101d8575f80fd5b806306fdde031461012b578063095ea7b31461014957806318160ddd1461016c5780631c8fc2c01461017e57806323b872dd14610186575b5f80fd5b61013361029a565b6040516101409190610b24565b60405180910390f35b61015c610157366004610b8a565b61032a565b6040519015158152602001610140565b6002545b604051908152602001610140565b610170600181565b61015c610194366004610bb2565b610343565b6101ac6101a7366004610c88565b610366565b005b60405160128152602001610140565b610170600281565b61015c6101d3366004610b8a565b6103f7565b6101705f81565b6101706101ed366004610cf0565b6001600160a01b03165f9081526020819052604090205490565b6101ac610418565b6005546040516001600160a01b039091168152602001610140565b61013361042b565b61015c610240366004610b8a565b61043a565b61015c610253366004610b8a565b6104b4565b61017060065481565b6101ac61026f366004610d10565b6104c1565b610170610282366004610d27565b6104da565b6101ac610295366004610cf0565b610504565b6060600380546102a990610d58565b80601f01602080910402602001604051908101604052809291908181526020018280546102d590610d58565b80156103205780601f106102f757610100808354040283529160200191610320565b820191905f5260205f20905b81548152906001019060200180831161030357829003601f168201915b5050505050905090565b5f3361033781858561057a565b60019150505b92915050565b5f3361035085828561069d565b61035b858585610715565b506001949350505050565b61036e6108c2565b60075460ff16156103bb5760405162461bcd60e51b8152602060048201526012602482015271151bdad95b8e881a5b9a5d1a585b1a5e995960721b60448201526064015b60405180910390fd5b6007805460ff191660011790556103d2838361091c565b6103ed6103e76005546001600160a01b031690565b8261093a565b5050600160065550565b5f3361033781858561040983836104da565b6104139190610d90565b61057a565b6104206108c2565b6104295f610a02565b565b6060600480546102a990610d58565b5f338161044782866104da565b9050838110156104a75760405162461bcd60e51b815260206004820152602560248201527f45524332303a2064656372656173656420616c6c6f77616e63652062656c6f77604482015264207a65726f60d81b60648201526084016103b2565b61035b828686840361057a565b5f33610337818585610715565b6104c96108c2565b600654156104d75760068190555b50565b6001600160a01b039182165f90815260016020908152604080832093909416825291909152205490565b61050c6108c2565b6001600160a01b0381166105715760405162461bcd60e51b815260206004820152602660248201527f4f776e61626c653a206e6577206f776e657220697320746865207a65726f206160448201526564647265737360d01b60648201526084016103b2565b6104d781610a02565b6001600160a01b0383166105dc5760405162461bcd60e51b8152602060048201526024808201527f45524332303a20617070726f76652066726f6d20746865207a65726f206164646044820152637265737360e01b60648201526084016103b2565b6001600160a01b03821661063d5760405162461bcd60e51b815260206004820152602260248201527f45524332303a20617070726f766520746f20746865207a65726f206164647265604482015261737360f01b60648201526084016103b2565b6001600160a01b038381165f8181526001602090815260408083209487168084529482529182902085905590518481527f8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925910160405180910390a3505050565b5f6106a884846104da565b90505f19811461070f57818110156107025760405162461bcd60e51b815260206004820152601d60248201527f45524332303a20696e73756666696369656e7420616c6c6f77616e636500000060448201526064016103b2565b61070f848484840361057a565b50505050565b6001600160a01b0383166107795760405162461bcd60e51b815260206004820152602560248201527f45524332303a207472616e736665722066726f6d20746865207a65726f206164604482015264647265737360d81b60648201526084016103b2565b6001600160a01b0382166107db5760405162461bcd60e51b815260206004820152602360248201527f45524332303a207472616e7366657220746f20746865207a65726f206164647260448201526265737360e81b60648201526084016103b2565b6107e6838383610a53565b6001600160a01b0383165f908152602081905260409020548181101561085d5760405162461bcd60e51b815260206004820152602660248201527f45524332303a207472616e7366657220616d6f756e7420657863656564732062604482015265616c616e636560d01b60648201526084016103b2565b6001600160a01b038481165f81815260208181526040808320878703905593871680835291849020805487019055925185815290927fddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef910160405180910390a361070f565b6005546001600160a01b031633146104295760405162461bcd60e51b815260206004820181905260248201527f4f776e61626c653a2063616c6c6572206973206e6f7420746865206f776e657260448201526064016103b2565b60036109288382610dfc565b5060046109358282610dfc565b505050565b6001600160a01b0382166109905760405162461bcd60e51b815260206004820152601f60248201527f45524332303a206d696e7420746f20746865207a65726f20616464726573730060448201526064016103b2565b61099b5f8383610a53565b8060025f8282546109ac9190610d90565b90915550506001600160a01b0382165f81815260208181526040808320805486019055518481527fddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef910160405180910390a35050565b600580546001600160a01b038381166001600160a01b0319831681179093556040519116919082907f8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e0905f90a35050565b600160065403610aa55760405162461bcd60e51b815260206004820152601d60248201527f546f6b656e3a205472616e73666572206973207265737472696374656400000060448201526064016103b2565b600260065403610935576005546001600160a01b0384811691161480610ad857506005546001600160a01b038381169116145b6109355760405162461bcd60e51b815260206004820152601760248201527f546f6b656e3a20496e76616c6964207472616e7366657200000000000000000060448201526064016103b2565b5f6020808352835180828501525f5b81811015610b4f57858101830151858201604001528201610b33565b505f604082860101526040601f19601f8301168501019250505092915050565b80356001600160a01b0381168114610b85575f80fd5b919050565b5f8060408385031215610b9b575f80fd5b610ba483610b6f565b946020939093013593505050565b5f805f60608486031215610bc4575f80fd5b610bcd84610b6f565b9250610bdb60208501610b6f565b9150604084013590509250925092565b634e487b7160e01b5f52604160045260245ffd5b5f82601f830112610c0e575f80fd5b813567ffffffffffffffff80821115610c2957610c29610beb565b604051601f8301601f19908116603f01168101908282118183101715610c5157610c51610beb565b81604052838152866020858801011115610c69575f80fd5b836020870160208301375f602085830101528094505050505092915050565b5f805f60608486031215610c9a575f80fd5b833567ffffffffffffffff80821115610cb1575f80fd5b610cbd87838801610bff565b94506020860135915080821115610cd2575f80fd5b50610cdf86828701610bff565b925050604084013590509250925092565b5f60208284031215610d00575f80fd5b610d0982610b6f565b9392505050565b5f60208284031215610d20575f80fd5b5035919050565b5f8060408385031215610d38575f80fd5b610d4183610b6f565b9150610d4f60208401610b6f565b90509250929050565b600181811c90821680610d6c57607f821691505b602082108103610d8a57634e487b7160e01b5f52602260045260245ffd5b50919050565b8082018082111561033d57634e487b7160e01b5f52601160045260245ffd5b601f821115610935575f81815260208120601f850160051c81016020861015610dd55750805b601f850160051c820191505b81811015610df457828155600101610de1565b505050505050565b815167ffffffffffffffff811115610e1657610e16610beb565b610e2a81610e248454610d58565b84610daf565b602080601f831160018114610e5d575f8415610e465750858301515b5f19600386901b1c1916600185901b178555610df4565b5f85815260208120601f198616915b82811015610e8b57888601518255948401946001909101908401610e6c565b5085821015610ea857878501515f19600388901b60f8161c191681555b5050505050600190811b0190555056fea26469706673582212203874a68f141da9e7322cb8fa8158cb6e06dacbfbfbf6320d1070ee24cf24335864736f6c63430008140033"
    ).expect("valid hex");

    keccak256(&init_code)
}

/// Compute CREATE2 address
/// Formula: keccak256(0xff ++ factory ++ salt ++ init_code_hash)[12:]
pub fn compute_create2_address(factory: Address, salt: B256, init_code_hash: B256) -> Address {
    let mut data = [0u8; 85];
    data[0] = 0xff;
    data[1..21].copy_from_slice(factory.as_slice());
    data[21..53].copy_from_slice(salt.as_slice());
    data[53..85].copy_from_slice(init_code_hash.as_slice());

    let hash = keccak256(&data);
    Address::from_slice(&hash[12..])
}

/// Four.meme main contract address
pub const FOUR_MEME_FACTORY: [u8; 20] = [
    0x5c, 0x95, 0x20, 0x63, 0xc7, 0xfc, 0x86, 0x10, 0xFF, 0xDB, 0x79, 0x81, 0x52, 0xD6, 0x9F, 0x0B,
    0x95, 0x50, 0x76, 0x2b,
];

/// Token deployer contract (CREATE2 factory) - this is the actual contract that deploys tokens
pub const TOKEN_DEPLOYER: [u8; 20] = [
    0x75, 0x7e, 0xba, 0x15, 0xa6, 0x44, 0x68, 0xe6, 0x53, 0x55, 0x32, 0xfc, 0xf0, 0x93, 0xce, 0xf9,
    0x0e, 0x22, 0x6f, 0x85,
];

/// Decoded parameters from createToken(bytes data, bytes signature) calldata
#[derive(Debug, Clone)]
pub struct CreateTokenParams {
    pub name: String,
    pub symbol: String,
    pub total_supply: U256,
    pub max_raising: U256,
    pub deadline: u64,
    pub fee1: U256, // listing fee
    pub fee2: U256, // trading fee
    pub nonce: u64, // unique token ID/nonce
}

/// ECDSA signature (65 bytes: r + s + v)
#[derive(Debug, Clone)]
pub struct Signature {
    pub r: [u8; 32],
    pub s: [u8; 32],
    pub v: u8,
}

/// Full decoded createToken call
#[derive(Debug, Clone)]
pub struct DecodedCreateToken {
    pub params: CreateTokenParams,
    pub signature: Signature,
}

/// Decode createToken(bytes,bytes) calldata
/// Selector: 0x519ebb10
pub fn decode_create_token_calldata(calldata: &[u8]) -> Option<DecodedCreateToken> {
    // Minimum: 4 (selector) + 64 (offsets) + data
    if calldata.len() < 68 {
        warn!("Calldata too short: {}", calldata.len());
        return None;
    }

    // Verify selector
    if &calldata[0..4] != &[0x51, 0x9e, 0xbb, 0x10] {
        warn!(
            "Invalid createToken selector: {:02x}{:02x}{:02x}{:02x}",
            calldata[0], calldata[1], calldata[2], calldata[3]
        );
        return None;
    }

    let data = &calldata[4..];

    // Read offsets to dynamic params
    let data_offset = read_u256(data, 0).to::<usize>();
    let sig_offset = read_u256(data, 32).to::<usize>();

    // Read data bytes
    let data_len = read_u256(data, data_offset).to::<usize>();
    let data_start = data_offset + 32;
    if data_start + data_len > data.len() {
        warn!(
            "Data bytes overflow: start={}, len={}, data.len={}",
            data_start,
            data_len,
            data.len()
        );
        return None;
    }
    let data_bytes = &data[data_start..data_start + data_len];

    // Read signature bytes
    let sig_len = read_u256(data, sig_offset).to::<usize>();
    let sig_start = sig_offset + 32;
    if sig_start + sig_len > data.len() || sig_len != 65 {
        warn!(
            "Invalid signature: len={}, start={}, data.len={}",
            sig_len,
            sig_start,
            data.len()
        );
        return None;
    }
    let sig_bytes = &data[sig_start..sig_start + 65];

    // Parse inner data structure
    let params = parse_inner_data(data_bytes)?;

    // Parse signature
    let signature = Signature {
        r: sig_bytes[0..32].try_into().ok()?,
        s: sig_bytes[32..64].try_into().ok()?,
        v: sig_bytes[64],
    };

    Some(DecodedCreateToken { params, signature })
}

/// Parse the inner data bytes structure
/// Layout based on decoded example:
/// - 0x00: inner offset (usually 0x20)
/// - 0x20: nonce (packed in first bytes of large value)
/// - 0x40: packed value (creator + extra)
/// - 0x60: name offset (0x200)
/// - 0x80: symbol offset (0x240)
/// - 0xa0: total_supply
/// - 0xc0: max_raising
/// - 0xe0: zero
/// - 0x100: zero
/// - 0x120: zero
/// - 0x140: fee1
/// - 0x160: fee2
/// - 0x180: deadline1
/// - 0x1a0: zero
/// - 0x1c0: zero
/// - 0x1e0: deadline2/timestamp2
/// - 0x200+: name (length + utf8)
/// - 0x240+: symbol (length + utf8)
fn parse_inner_data(data: &[u8]) -> Option<CreateTokenParams> {
    if data.len() < 0x260 {
        warn!(
            "Inner data too short: {} bytes (need >= {})",
            data.len(),
            0x260
        );
        return None;
    }

    // Read inner offset - struct data starts at this position (usually 0x20)
    let inner_offset = read_u256(data, 0x00).to::<usize>();
    if inner_offset != 0x20 {
        warn!("Unexpected inner offset: {:#x}", inner_offset);
        // Continue anyway, might still work
    }

    // Extract nonce from slot at 0x20 (first 8 bytes often contain nonce)
    let nonce_slot = read_u256(data, 0x20);
    let shifted: U256 = nonce_slot >> 224;
    let nonce = shifted.to::<u64>(); // Top 4 bytes as nonce

    // Name and symbol offsets (relative to inner_offset position)
    let name_rel_offset = read_u256(data, 0x60).to::<usize>();
    let symbol_rel_offset = read_u256(data, 0x80).to::<usize>();

    // Actual positions = inner_offset + relative_offset
    let name_offset = inner_offset + name_rel_offset;
    let symbol_offset = inner_offset + symbol_rel_offset;

    // Token economics
    let total_supply = read_u256(data, 0xa0);
    let max_raising = read_u256(data, 0xc0);

    // Fees
    let fee1 = read_u256(data, 0x140);
    let fee2 = read_u256(data, 0x160);

    // Deadline
    let deadline = read_u256(data, 0x180).to::<u64>();

    // Decode name string
    let name = match decode_string_at(data, name_offset) {
        Some(n) => n,
        None => {
            warn!(
                "Failed to decode name at offset {:#x}, data.len={}",
                name_offset,
                data.len()
            );
            return None;
        }
    };

    // Decode symbol string
    let symbol = match decode_string_at(data, symbol_offset) {
        Some(s) => s,
        None => {
            warn!(
                "Failed to decode symbol at offset {:#x}, data.len={}",
                symbol_offset,
                data.len()
            );
            return None;
        }
    };

    Some(CreateTokenParams {
        name,
        symbol,
        total_supply,
        max_raising,
        deadline,
        fee1,
        fee2,
        nonce,
    })
}

/// Read U256 from data at byte offset
fn read_u256(data: &[u8], offset: usize) -> U256 {
    if offset + 32 > data.len() {
        return U256::ZERO;
    }
    U256::from_be_slice(&data[offset..offset + 32])
}

/// Decode length-prefixed UTF-8 string at offset
fn decode_string_at(data: &[u8], offset: usize) -> Option<String> {
    if offset + 32 > data.len() {
        warn!(
            "String offset out of bounds: offset={:#x}, data.len={}",
            offset,
            data.len()
        );
        return None;
    }

    let length = read_u256(data, offset).to::<usize>();
    if length == 0 || length > 256 {
        warn!("Invalid string length: {} at offset {:#x}", length, offset);
        return None;
    }

    let str_start = offset + 32;
    if str_start + length > data.len() {
        warn!(
            "String content overflow: start={}, len={}, data.len={}",
            str_start,
            length,
            data.len()
        );
        return None;
    }

    match String::from_utf8(data[str_start..str_start + length].to_vec()) {
        Ok(s) => Some(s),
        Err(e) => {
            warn!("UTF-8 decode error: {}", e);
            None
        }
    }
}

/// Quick extraction of just name and symbol (for existing code compatibility)
pub fn extract_name_symbol(calldata: &[u8]) -> Option<(String, String)> {
    let decoded = decode_create_token_calldata(calldata)?;
    Some((decoded.params.name, decoded.params.symbol))
}

/// Extract calldata from a raw EIP-1559 transaction (type 0x02)
/// Returns the data field from the transaction
pub fn extract_calldata_from_raw_tx(raw_tx: &[u8]) -> Option<Vec<u8>> {
    if raw_tx.is_empty() {
        return None;
    }

    // Check for EIP-1559 type (0x02)
    if raw_tx[0] != 0x02 {
        warn!("Not an EIP-1559 transaction, type: {:#x}", raw_tx[0]);
        return None;
    }

    // Skip tx type byte
    let rlp_data = &raw_tx[1..];

    // Parse RLP list header
    if rlp_data.is_empty() {
        return None;
    }

    let (list_offset, _list_len) = parse_rlp_length(rlp_data)?;
    let list_data = &rlp_data[list_offset..];

    // EIP-1559 tx fields: [chainId, nonce, maxPriorityFeePerGas, maxFeePerGas, gasLimit, to, value, data, accessList, ...]
    // We need to skip to field 7 (data, 0-indexed)
    let mut offset = 0;
    for field_idx in 0..8 {
        if offset >= list_data.len() {
            return None;
        }

        let (item_header_len, item_len) = parse_rlp_length(&list_data[offset..])?;

        if field_idx == 7 {
            // This is the data field
            let data_start = offset + item_header_len;
            let data_end = data_start + item_len;
            if data_end > list_data.len() {
                return None;
            }
            return Some(list_data[data_start..data_end].to_vec());
        }

        offset += item_header_len + item_len;
    }

    None
}

/// Parse RLP length prefix, returns (header_length, data_length)
fn parse_rlp_length(data: &[u8]) -> Option<(usize, usize)> {
    if data.is_empty() {
        return None;
    }

    let first = data[0];

    if first <= 0x7f {
        // Single byte
        Some((0, 1))
    } else if first <= 0xb7 {
        // Short string (0-55 bytes)
        let len = (first - 0x80) as usize;
        Some((1, len))
    } else if first <= 0xbf {
        // Long string
        let len_of_len = (first - 0xb7) as usize;
        if data.len() < 1 + len_of_len {
            return None;
        }
        let mut len = 0usize;
        for i in 0..len_of_len {
            len = (len << 8) | (data[1 + i] as usize);
        }
        Some((1 + len_of_len, len))
    } else if first <= 0xf7 {
        // Short list (0-55 bytes)
        let len = (first - 0xc0) as usize;
        Some((1, len))
    } else {
        // Long list
        let len_of_len = (first - 0xf7) as usize;
        if data.len() < 1 + len_of_len {
            return None;
        }
        let mut len = 0usize;
        for i in 0..len_of_len {
            len = (len << 8) | (data[1 + i] as usize);
        }
        Some((1 + len_of_len, len))
    }
}

/// Predict token address from raw transaction bytes
pub fn predict_token_address_from_raw_tx(raw_tx: &[u8]) -> Option<Address> {
    let calldata = extract_calldata_from_raw_tx(raw_tx)?;
    predict_token_address(&calldata)
}

/// Predict the token address from createToken calldata using CREATE2
/// Returns the token address that will be deployed when this tx is mined
pub fn predict_token_address(calldata: &[u8]) -> Option<Address> {
    // Verify selector
    if calldata.len() < 68 || &calldata[0..4] != &[0x51, 0x9e, 0xbb, 0x10] {
        return None;
    }

    let data = &calldata[4..];
    let data_offset = U256::from_be_slice(&data[0..32]).to::<usize>();

    if data_offset + 32 > data.len() {
        return None;
    }

    let data_len = U256::from_be_slice(&data[data_offset..data_offset + 32]).to::<usize>();
    let data_start = data_offset + 32;

    if data_start + data_len > data.len() {
        return None;
    }

    let data_bytes = &data[data_start..data_start + data_len];

    // Salt is at offset 0x40 in the inner data (32 bytes)
    if data_bytes.len() < 0x60 {
        return None;
    }

    let salt = B256::from_slice(&data_bytes[0x40..0x60]);
    let factory = Address::from(TOKEN_DEPLOYER);

    Some(compute_create2_address(
        factory,
        salt,
        *CACHED_INIT_CODE_HASH,
    ))
}

/// Format token params for logging
impl std::fmt::Display for CreateTokenParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let supply_formatted = format_token_amount(&self.total_supply);
        let raising_formatted = format_bnb_amount(&self.max_raising);
        let fee1_formatted = format_bnb_amount(&self.fee1);
        let fee2_formatted = format_bnb_amount(&self.fee2);

        write!(
            f,
            "Token: {} ({}) | Supply: {} | MaxRaise: {} BNB | Fees: {}/{} BNB | Deadline: {}",
            self.name,
            self.symbol,
            supply_formatted,
            raising_formatted,
            fee1_formatted,
            fee2_formatted,
            self.deadline
        )
    }
}

/// Format U256 as token amount (divide by 10^18)
fn format_token_amount(amount: &U256) -> String {
    let decimals = U256::from(10).pow(U256::from(18));
    let whole = *amount / decimals;
    if whole >= U256::from(1_000_000_000u64) {
        format!("{}B", whole / U256::from(1_000_000_000u64))
    } else if whole >= U256::from(1_000_000u64) {
        format!("{}M", whole / U256::from(1_000_000u64))
    } else if whole >= U256::from(1_000u64) {
        format!("{}K", whole / U256::from(1_000u64))
    } else {
        format!("{}", whole)
    }
}

/// Format U256 as BNB amount (divide by 10^18)
fn format_bnb_amount(amount: &U256) -> String {
    let decimals = U256::from(10).pow(U256::from(18));
    let wei_per_gwei = U256::from(1_000_000_000u64);

    if *amount >= decimals {
        let whole = *amount / decimals;
        let frac = (*amount % decimals) / (decimals / U256::from(100u64));
        format!("{}.{:02}", whole, frac.to::<u64>())
    } else if *amount >= wei_per_gwei {
        let gwei = *amount / wei_per_gwei;
        format!("0.{:09}", gwei.to::<u64>())
    } else {
        format!("{} wei", amount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_example_calldata() {
        // Example from user - BSC meme token createToken calldata
        let hex_data = "519ebb100000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000030000000000000000000000000000000000000000000000000000000000000002a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000060e83860000019bd5f3e71200000000000000000000000000000000000000001478d70d000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000002400000000000000000000000000000000000000000033b2e3c9fd0803ce800000000000000000000000000000000000000000000000295be96e640669720000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000f9ccd8a1c50800000000000000000000000000000000000000000000000000000c3663566a58000000000000000000000000000000000000000000000000000000000000696f517c0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000696f517200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000012e5b881e5ae89e6af9be7bb92e78ea9e585b700000000000000000000000000000000000000000000000000000000000000000000000000000000000000000012e5b881e5ae89e6af9be7bb92e78ea9e585b700000000000000000000000000000000000000000000000000000000000000000000000000000000000000000041abe7c64adbdb3c2777d6ef124b9f14c32393d74cb71380a3ac20a6681dc6d7d06914900b14fdd762a958bb1260f82c20c3bb23948821112aa684fe5a239ae1931b00000000000000000000000000000000000000000000000000000000000000";

        let calldata = hex::decode(hex_data).unwrap();
        let decoded = decode_create_token_calldata(&calldata).unwrap();

        // Verify decoded values
        assert_eq!(decoded.params.name, "币安毛绒玩具"); // "Binance Plush Toy" in Chinese
        assert_eq!(decoded.params.symbol, "币安毛绒玩具");
        assert_eq!(decoded.params.deadline, 1768903036); // ~Jan 20, 2026
        assert_eq!(decoded.signature.v, 0x1b); // 27

        // Verify token economics
        assert!(decoded.params.total_supply > U256::ZERO);
        assert!(decoded.params.max_raising > U256::ZERO);

        println!("✓ Decoded: {}", decoded.params);
    }

    #[test]
    fn test_init_code_hash() {
        let hash = compute_init_code_hash();
        println!("Init code hash: {:#x}", hash);

        // Verify the hash is non-zero
        assert_ne!(hash, B256::ZERO);
    }

    #[test]
    fn test_predict_token_address() {
        use std::str::FromStr;

        let hex_data = "519ebb100000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000030000000000000000000000000000000000000000000000000000000000000002a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000060e83860000019bd5f3e71200000000000000000000000000000000000000001478d70d000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000002400000000000000000000000000000000000000000033b2e3c9fd0803ce800000000000000000000000000000000000000000000000295be96e640669720000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000f9ccd8a1c50800000000000000000000000000000000000000000000000000000c3663566a58000000000000000000000000000000000000000000000000000000000000696f517c0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000696f517200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000012e5b881e5ae89e6af9be7bb92e78ea9e585b700000000000000000000000000000000000000000000000000000000000000000000000000000000000000000012e5b881e5ae89e6af9be7bb92e78ea9e585b700000000000000000000000000000000000000000000000000000000000000000000000000000000000000000041abe7c64adbdb3c2777d6ef124b9f14c32393d74cb71380a3ac20a6681dc6d7d06914900b14fdd762a958bb1260f82c20c3bb23948821112aa684fe5a239ae1931b00000000000000000000000000000000000000000000000000000000000000";

        let calldata = hex::decode(hex_data).unwrap();
        let expected = Address::from_str("0xc42b2ad73b94eaf6c252987b48d93e18b5084444").unwrap();

        let predicted = predict_token_address(&calldata).expect("should predict address");

        println!("Predicted: {:?}", predicted);
        println!("Expected:  {:?}", expected);

        assert_eq!(
            predicted, expected,
            "Predicted address should match expected"
        );
    }

    #[test]
    fn test_predict_from_raw_tx() {
        use std::str::FromStr;

        // Raw EIP-1559 transaction for "test/TEST" token
        let raw_tx_hex = "02f903f838028402faf0808402faf0808320bc02945c952063c7fc8610ffdb798152d69f0b9550762b872386f26fc10000b90384519ebb100000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000030000000000000000000000000000000000000000000000000000000000000002a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000060f25d80000019bea3e9ae2000000000000000000000000000000000000000011e8a41a000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000002400000000000000000000000000000000000000000033b2e3c9fd0803ce800000000000000000000000000000000000000000000000295be96e640669720000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000f9ccd8a1c50800000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000006973dc3b00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000006973dc31000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000047465737400000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000454455354000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000041cac2d9e994188c869bac06a856e20a5bda58ee21b87281dcba1c5f8793b7d454430532313abc0fabae679723a7a8394db88b37eb3fd24ecf9188c50e8e9a88961b00000000000000000000000000000000000000000000000000000000000000c080a0f9d283813faf07ff6dbe091fbfa7efab9f0ee7de5c97e1d5d6336e21fe35011fa05dbe2336c9b5c37c2c27cdc9334ee068632e5c3a1e8ef2b56d3bfccc9c45324c";

        let raw_tx = hex::decode(raw_tx_hex).unwrap();
        let expected = Address::from_str("0xd0dc9f0699118dfe3d04a48e931b008933c14444").unwrap();

        // First, verify we can extract the calldata
        let calldata = extract_calldata_from_raw_tx(&raw_tx).expect("should extract calldata");
        println!("Extracted calldata length: {}", calldata.len());
        println!(
            "Calldata starts with: {:02x}{:02x}{:02x}{:02x}",
            calldata[0], calldata[1], calldata[2], calldata[3]
        );

        // Verify it's a createToken call
        assert_eq!(
            &calldata[0..4],
            &[0x51, 0x9e, 0xbb, 0x10],
            "Should be createToken selector"
        );

        // Decode to verify token name
        let decoded = decode_create_token_calldata(&calldata).expect("should decode");
        println!("Token name: {}", decoded.params.name);
        println!("Token symbol: {}", decoded.params.symbol);

        // Predict token address
        let predicted = predict_token_address_from_raw_tx(&raw_tx).expect("should predict");
        println!("Predicted: {:?}", predicted);
        println!("Expected:  {:?}", expected);

        assert_eq!(
            predicted, expected,
            "Predicted address should match expected"
        );
    }
}
