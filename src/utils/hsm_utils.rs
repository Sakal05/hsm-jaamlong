use anyhow::Error;
use rlp::RlpStream;
use secp256k1::SecretKey;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::types::Json;
use std::str::FromStr;
use web3::{
    api::{Eth, Namespace},
    contract::tokens::Tokenize,
    ethabi::Token,
    signing,
    signing::Signature,
    transports::Http,
    types::{
        AccessList, Address, Bytes, SignedTransaction, TransactionParameters, H160, U256, U64,
    },
};

const LEGACY_TX_ID: u64 = 0;
const ACCESSLISTS_TX_ID: u64 = 1;
const EIP1559_TX_ID: u64 = 2;

#[derive(Debug)]
pub struct TransactionParam {
    pub to: Option<Address>,
    pub nonce: U256,
    pub gas: U256,
    pub gas_price: U256,
    pub value: U256,
    pub data: Vec<u8>,
    pub transaction_type: Option<U64>,
    pub access_list: AccessList,
    pub max_priority_fee_per_gas: U256,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TxRequest {
    pub chain_id: String,
    pub to: String,
    pub nonce: String,
    pub value: String,
    pub gas: String,
    pub gas_price: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TxBroadcastRequest {
    pub network_rpc: String,
    pub bridge_address: String,
    pub tx: TxRequest,
    pub token_address: Option<String>,
    pub abi: Option<Json<Value>>,
}

pub async fn sign_erc20(transaction: &TxBroadcastRequest) -> Result<Bytes, Error> {
    //========== implement authorization checks
    let token_address = match &transaction.token_address {
        Some(token) => token,
        None => return Err(Error::msg("Token Address Not Found")),
    };
    let abi = match &transaction.abi {
        Some(abi) => abi,
        None => return Err(Error::msg("Token Address Not Found")),
    };
    println!("Receive tx: {:#?}", &transaction);
    let transport = match web3::transports::Http::new(&transaction.network_rpc) {
        Ok(transport) => transport,
        Err(err) => return Err(Error::msg(format!("Error Initialize Transport: {}", err))),
    };
    let contract = match contract(
        transport.clone(),
        H160::from_str(token_address).unwrap(),
        abi,
    )
    .await
    {
        Ok(contract) => contract,
        Err(err) => return Err(Error::msg(format!("Error: {}", err))),
    };
    let actual_transfer_amount = U256::from_dec_str(&transaction.tx.value).unwrap();
    println!("Actual Transfer Amount: {}", actual_transfer_amount);
    let nonce = U256::from_dec_str(&transaction.tx.nonce).unwrap();
    let gas_price = U256::from_dec_str(&transaction.tx.gas_price).unwrap();
    let gas = U256::from_dec_str(&transaction.tx.gas).unwrap();
    let receiver_address = match H160::from_str(&transaction.tx.to) {
        Ok(address) => address,
        Err(err) => {
            return Err(Error::msg(format!(
                "Error parsing receiver address: {}",
                err
            )))
        }
    };
    let params = (
        Token::Address(receiver_address),
        Token::Uint(actual_transfer_amount),
    );
    let fn_data = contract
        .abi()
        .function("transfer")
        .and_then(|function| function.encode_input(&params.into_tokens()))
        // TODO [ToDr] SendTransactionWithConfirmation should support custom error type (so that we can return
        // `contract::Error` instead of more generic `Error`.
        .map_err(|err| web3::Error::Decoder(format!("Error: {}", err)))?;
    let tx_p = TransactionParameters {
        nonce: Some(nonce),
        to: Some(contract.address()),
        gas,
        gas_price: Some(gas_price),
        data: Bytes(fn_data),
        ..Default::default()
    };
    println!("Tx P: {:#?}", tx_p);
    let max_priority_fee_per_gas = match tx_p.transaction_type {
        Some(tx_type) if tx_type == U64::from(EIP1559_TX_ID) => {
            tx_p.max_priority_fee_per_gas.unwrap_or(gas_price)
        }
        _ => gas_price,
    };
    let tx = TransactionParam {
        to: Some(tx_p.to.unwrap()),
        nonce,
        gas: tx_p.gas,
        gas_price: tx_p.gas_price.unwrap(),
        value: tx_p.value,
        data: tx_p.data.0,
        transaction_type: tx_p.transaction_type,
        access_list: tx_p.access_list.unwrap_or_default(),
        max_priority_fee_per_gas,
    };
    println!("Tx Param: {:#?}", &tx);
    let private_key = dotenvy::var("PRIVATE_KEY").expect("Private key must be set");
    let key = match SecretKey::from_str(&private_key) {
        Ok(k) => k,
        Err(err) => return Err(Error::msg(format!("Error parsing key: {}", err))),
    };
    let sign_tx = sign_raw(&tx, &key, u64::from_str(&transaction.tx.chain_id).unwrap());
    println!("Signed Tx: {:#?}", sign_tx);
    Ok(sign_tx.raw_transaction)
}

pub async fn sign_raw_tx(transaction: &TxBroadcastRequest) -> Result<Bytes, Error> {
    let actual_transfer_amount = U256::from_dec_str(&transaction.tx.value).unwrap();
    println!("Actual Transfer Amount: {}", actual_transfer_amount);
    let nonce = U256::from_dec_str(&transaction.tx.nonce).unwrap();
    let gas_price = U256::from_dec_str(&transaction.tx.gas_price).unwrap();
    let gas = U256::from_dec_str(&transaction.tx.gas).unwrap();
    let receiver_address = match H160::from_str(&transaction.tx.to) {
        Ok(address) => address,
        Err(err) => {
            return Err(Error::msg(format!(
                "Error parsing receiver address: {}",
                err
            )))
        }
    };
    let tx_p = TransactionParameters {
        nonce: Some(nonce),
        to: Some(receiver_address),
        value: actual_transfer_amount,
        gas,
        gas_price: Some(gas_price),
        ..Default::default()
    };
    let max_priority_fee_per_gas = match tx_p.transaction_type {
        Some(tx_type) if tx_type == U64::from(EIP1559_TX_ID) => {
            tx_p.max_priority_fee_per_gas.unwrap_or(gas_price)
        }
        _ => gas_price,
    };
    let tx = TransactionParam {
        to: Some(tx_p.to.unwrap()),
        nonce,
        gas: tx_p.gas,
        gas_price: tx_p.gas_price.unwrap(),
        value: tx_p.value,
        data: tx_p.data.0,
        transaction_type: tx_p.transaction_type,
        access_list: tx_p.access_list.unwrap_or_default(),
        max_priority_fee_per_gas,
    };
    println!("Tx Param: {:#?}", &tx);
    let private_key = dotenvy::var("PRIVATE_KEY").expect("Private key must be set");
    let key = match SecretKey::from_str(&private_key) {
        Ok(k) => k,
        Err(err) => return Err(Error::msg(format!("Error parsing key: {}", err))),
    };
    let sign_tx = sign_raw(&tx, &key, u64::from_str(&transaction.tx.chain_id).unwrap());
    println!("Signed Tx: {:#?}", sign_tx);
    Ok(sign_tx.raw_transaction)
}

fn rlp_append_legacy(tx: &TransactionParam, stream: &mut RlpStream) {
    stream.append(&tx.nonce);
    stream.append(&tx.gas_price);
    stream.append(&tx.gas);
    if let Some(to) = tx.to {
        stream.append(&to);
    } else {
        stream.append(&"");
    }
    stream.append(&tx.value);
    stream.append(&tx.data);
}

fn encode_legacy(tx: &TransactionParam, chain_id: u64, signature: Option<&Signature>) -> RlpStream {
    let mut stream = RlpStream::new();
    stream.begin_list(9);

    rlp_append_legacy(tx, &mut stream);

    if let Some(signature) = signature {
        rlp_append_signature(&mut stream, signature);
    } else {
        stream.append(&chain_id);
        stream.append(&0u8);
        stream.append(&0u8);
    }

    stream
}

fn encode_access_list_payload(
    tx: &TransactionParam,
    chain_id: u64,
    signature: Option<&Signature>,
) -> RlpStream {
    let mut stream = RlpStream::new();

    let list_size = if signature.is_some() { 11 } else { 8 };
    stream.begin_list(list_size);

    // append chain_id. from EIP-2930: chainId is defined to be an integer of arbitrary size.
    stream.append(&chain_id);

    rlp_append_legacy(tx, &mut stream);
    rlp_append_access_list(tx, &mut stream);

    if let Some(signature) = signature {
        rlp_append_signature(&mut stream, signature);
    }

    stream
}

fn encode_eip1559_payload(
    tx: &TransactionParam,
    chain_id: u64,
    signature: Option<&Signature>,
) -> RlpStream {
    let mut stream = RlpStream::new();

    let list_size = if signature.is_some() { 12 } else { 9 };
    stream.begin_list(list_size);

    // append chain_id. from EIP-2930: chainId is defined to be an integer of arbitrary size.
    stream.append(&chain_id);

    stream.append(&tx.nonce);
    stream.append(&tx.max_priority_fee_per_gas);
    stream.append(&tx.gas_price);
    stream.append(&tx.gas);
    if let Some(to) = tx.to {
        stream.append(&to);
    } else {
        stream.append(&"");
    }
    stream.append(&tx.value);
    stream.append(&tx.data);

    rlp_append_access_list(tx, &mut stream);

    if let Some(signature) = signature {
        rlp_append_signature(&mut stream, signature);
    }

    stream
}

fn rlp_append_signature(stream: &mut RlpStream, signature: &Signature) {
    stream.append(&signature.v);
    stream.append(&U256::from_big_endian(signature.r.as_bytes()));
    stream.append(&U256::from_big_endian(signature.s.as_bytes()));
}

fn rlp_append_access_list(tx: &TransactionParam, stream: &mut RlpStream) {
    stream.begin_list(tx.access_list.clone().len());
    for access in tx.access_list.clone().iter() {
        stream.begin_list(2);
        stream.append(&access.address);
        stream.begin_list(access.storage_keys.len());
        for storage_key in access.storage_keys.iter() {
            stream.append(storage_key);
        }
    }
}

fn encode(tx: &TransactionParam, chain_id: u64, signature: Option<&Signature>) -> Vec<u8> {
    match tx.transaction_type.map(|t| t.as_u64()) {
        Some(LEGACY_TX_ID) | None => {
            let stream = encode_legacy(tx, chain_id, signature);
            stream.out().to_vec()
        }

        Some(ACCESSLISTS_TX_ID) => {
            let tx_id: u8 = ACCESSLISTS_TX_ID as u8;
            let stream = encode_access_list_payload(tx, chain_id, signature);
            [&[tx_id], stream.as_raw()].concat()
        }

        Some(EIP1559_TX_ID) => {
            let tx_id: u8 = EIP1559_TX_ID as u8;
            let stream = encode_eip1559_payload(tx, chain_id, signature);
            [&[tx_id], stream.as_raw()].concat()
        }

        _ => {
            panic!("Unsupported transaction type");
        }
    }
}

/// Sign and return a raw signed transaction.
fn sign_raw(tx: &TransactionParam, sign: impl signing::Key, chain_id: u64) -> SignedTransaction {
    let adjust_v_value = matches!(
        tx.transaction_type.map(|t| t.as_u64()),
        Some(LEGACY_TX_ID) | None
    );
    println!("======== Chain ID: {}", chain_id);
    let encoded = encode(tx, chain_id, None);

    let hash = signing::keccak256(encoded.as_ref());

    let signature = if adjust_v_value {
        sign.sign(&hash, Some(chain_id))
            .expect("hash is non-zero 32-bytes; qed")
    } else {
        sign.sign_message(&hash)
            .expect("hash is non-zero 32-bytes; qed")
    };

    let signed = encode(tx, chain_id, Some(&signature));
    let transaction_hash = signing::keccak256(signed.as_ref()).into();

    SignedTransaction {
        message_hash: hash.into(),
        v: signature.v,
        r: signature.r,
        s: signature.s,
        raw_transaction: signed.into(),
        transaction_hash,
    }
}

async fn contract(
    transport: Http,
    address: Address,
    abi_json: &Json<Value>,
) -> Result<web3::contract::Contract<Http>, Error> {
    let eth = Eth::new(transport);
    let abi: String = match serde_json::to_string(&abi_json) {
        Ok(abi) => abi,
        Err(err) => {
            return Err(err.into());
        }
    };
    let json_bytes = abi.as_bytes().to_vec();
    match web3::contract::Contract::from_json(eth, address, &json_bytes) {
        Ok(contract) => Ok(contract),
        Err(err) => Err(Error::msg(format!("Error Initialize Contract: {}", err))),
    }
}
