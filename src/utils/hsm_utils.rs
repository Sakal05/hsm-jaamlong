use anyhow::Error;
use hkdf::Hkdf;
use p256::{
    ecdh::EphemeralSecret, ecdsa::signature::Verifier, ecdsa::VerifyingKey, EncodedPoint, PublicKey,
};
use rand_core::OsRng;
use redis::Commands;
use rlp::RlpStream;
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
        AccessList, Address, Bytes, SignedTransaction, TransactionParameters, H160, H256, U256, U64,
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TxRequestTest {
    pub sign_tx: String,
    pub pk: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SignTx {
    pub message: Vec<u8>,
    pub signature: Vec<u8>,
    pub v_key: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SignRawTxFeild {
    pub message: [u8; 32],
    pub r_tx: Bytes,
    pub signature: Vec<u8>,
}

pub async fn sign_erc20(transaction: &TxBroadcastRequest) -> Result<SignRawTxFeild, Error> {
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
    // init key instance
    let private_key = dotenvy::var("PRIVATE_KEY").expect("Private key must be set");
    let key = match web3::signing::SecretKey::from_str(&private_key) {
        Ok(k) => k,
        Err(err) => return Err(Error::msg(format!("Error parsing key: {}", err))),
    };
    let sign_tx = sign_raw(&tx, &key, u64::from_str(&transaction.tx.chain_id).unwrap());
    let combined_sign_bytes = combined_sign_bytes(sign_tx.v, sign_tx.r, sign_tx.s);

    println!("Signed Tx: {:#?}", sign_tx);
    let sign_tx_field = SignRawTxFeild {
        message: sign_tx.message_hash.0,
        r_tx: sign_tx.raw_transaction,
        signature: combined_sign_bytes.to_vec(),
    };
    Ok(sign_tx_field)
}

pub async fn sign_raw_tx(transaction: &TxBroadcastRequest) -> Result<SignRawTxFeild, Error> {
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
    let key = match web3::signing::SecretKey::from_str(&private_key) {
        Ok(k) => k,
        Err(err) => return Err(Error::msg(format!("Error parsing key: {}", err))),
    };
    let sign_tx = sign_raw(&tx, &key, u64::from_str(&transaction.tx.chain_id).unwrap());
    let combined_sign_bytes = combined_sign_bytes(sign_tx.v, sign_tx.r, sign_tx.s);
    let sign_tx_field = SignRawTxFeild {
        message: sign_tx.message_hash.0,
        r_tx: sign_tx.raw_transaction,
        signature: combined_sign_bytes.to_vec(),
    };
    Ok(sign_tx_field)
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
    println!("Chain ID: {}", chain_id);
    println!("Adjusted value flag: {}", adjust_v_value);
    let encoded = encode(tx, chain_id, None);

    let hash = signing::keccak256(encoded.as_ref());

    let signature = if adjust_v_value {
        println!("Address Sign: {:#?}", sign.address());
        sign.sign(&hash, Some(chain_id))
            .expect("hash is non-zero 32-bytes; qed")
    } else {
        sign.sign_message(&hash)
            .expect("hash is non-zero 32-bytes; qed")
    };

    let signed = encode(tx, chain_id, Some(&signature));
    let transaction_hash: web3::types::H256 = signing::keccak256(signed.as_ref()).into();

    let s: SignedTransaction = SignedTransaction {
        message_hash: hash.into(),
        v: signature.v,
        r: signature.r,
        s: signature.s,
        raw_transaction: signed.into(),
        transaction_hash,
    };
    println!("{:#?}", &s);
    s
}

pub fn sign_message(message: &[u8], sign: impl signing::Key) -> [u8; 65] {
    let signature = sign.sign_message(message).unwrap();
    let combined_bytes: [u8; 65] = {
        let mut combined = [0u8; 65];
        combined[..32].copy_from_slice(&signature.r.0);
        combined[32..64].copy_from_slice(&signature.s.0);
        combined[64] = signature.v as u8;
        combined
    };
    combined_bytes
}

pub fn recover(recovery: web3::types::Recovery) -> anyhow::Result<Address> {
    // let recovery: web3::types::Recovery = recovery.into();
    let message_hash = match recovery.message {
        web3::types::RecoveryMessage::Data(ref message) => signing::hash_message(message),
        web3::types::RecoveryMessage::Hash(hash) => hash,
    };
    let (signature, recovery_id) = recovery.as_signature().ok_or(web3::Error::Recovery(
        signing::RecoveryError::InvalidSignature,
    ))?;
    let address = signing::recover(message_hash.as_bytes(), &signature, recovery_id)?;
    Ok(address)
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

pub fn verify_signature(sign_tx: &SignTx) -> bool {
    // ==== signature verifying process =====
    // ========= received payload: message (tx_field), signature, verification_bytes
    let received_v_key = VerifyingKey::from_sec1_bytes(&sign_tx.v_key).unwrap();
    let signature_parse = p256::ecdsa::Signature::from_slice(&sign_tx.signature).unwrap();

    let stage = received_v_key
        .verify(&sign_tx.message, &signature_parse.clone())
        .is_ok();
    println!("Stage of verification: {}", stage);
    stage
}

pub fn hsm_generate_pk(origin_pk: &[u8]) -> (Vec<u8>, EphemeralSecret) {
    let hsm_secret = EphemeralSecret::random(&mut OsRng);
    let hsm_pk_bytes = EncodedPoint::from(hsm_secret.public_key()).to_bytes();
    let _sk = generate_sk(origin_pk, &hsm_secret);
    println!("Generated PK HSM: {:?}", hsm_pk_bytes.to_vec());

    (hsm_pk_bytes.to_vec(), hsm_secret)
}

pub fn generate_sk(pk_bytes: &[u8], sk_bytes: &EphemeralSecret) -> Result<Vec<u8>, anyhow::Error> {
    let public = PublicKey::from_sec1_bytes(pk_bytes).expect("bob's public key is invalid!");

    let shared_key = sk_bytes.diffie_hellman(&public);

    let shared = shared_key.raw_secret_bytes();

    let client = redis::Client::open("redis://127.0.0.1:6379")?;
    let mut con = client.get_connection()?;

    let output_length = 32;
    let hkdf = Hkdf::<sha2::Sha256>::new(None, shared.as_slice());
    let info = pk_bytes;
    // Expand the shared key using the provided info and desired output length
    let mut okm = vec![0u8; output_length];
    let _ = hkdf.expand(info, &mut okm);

    con.set(pk_bytes, shared.to_vec())?;

    // let k: Vec<u8> = con.get(pk_bytes).expect("Key value not found");
    // println!("Key is {:#?}", k);

    println!("OKM: {:?}", okm);

    Ok(shared.to_vec())
}

fn combined_sign_bytes(v: u64, r: H256, s: H256) -> [u8; 65] {
    let mut combined = [0u8; 65];
    combined[..32].copy_from_slice(&r.0);
    combined[32..64].copy_from_slice(&s.0);
    combined[64] = v as u8;
    combined
}
