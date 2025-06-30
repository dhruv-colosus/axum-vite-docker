use axum::{
    routing::{get, post},
    Router,
    http::{Method, StatusCode},
    Json,
    response::Json as ResponseJson,
    extract::Query,
};
use tower_http::{
    services::ServeDir,
    cors::{CorsLayer},
};
use std::net::SocketAddr;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    signer::{keypair::Keypair, Signer},
    signature::Signature,
    native_token::LAMPORTS_PER_SOL,
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    instruction::Instruction,
    sysvar,
    system_instruction,
    system_program,
};
use spl_token::{instruction::initialize_mint2, ID as TOKEN_PROGRAM_ID};
use serde::{Deserialize, Serialize};
use base64;
use spl_associated_token_account::get_associated_token_address;
use spl_token::instruction::transfer_checked;

#[derive(Serialize)]
struct ApiResponse<T> {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl<T> ApiResponse<T> {
    fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    fn error(message: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message),
        }
    }
}

#[derive(Serialize)]
struct AirdropData {
    account_id: String,
    airdrop_signature: String,
}

#[derive(Serialize)]
struct BalanceData {
    balance: u64,
    public_key: String,
}

#[derive(Serialize)]
struct HelloData {
    message: String,
}

#[derive(Deserialize, Debug)]
struct BalanceRequest {
    public_key: String,
}

#[derive(Deserialize, Debug)]
struct BalanceQuery {
    public_key: String,
}

#[derive(Serialize)]
struct KeypairData {
    pubkey: String,
    secret: String,
}

#[derive(Deserialize, Debug)]
struct TokenCreateRequest {
    #[serde(rename = "mintAuthority")]
    mintAuthority: String,
    mint: String,
    decimals: u8,
}

#[derive(Serialize)]
struct Account {
    pubkey: String,
    is_signer: bool,
    is_writable: bool,
}

#[derive(Serialize)]
struct TokenCreateData {
    program_id: String,
    accounts: Vec<Account>,
    instruction_data: String,
}

#[derive(Deserialize, Debug)]
struct MessageSignRequest {
    message: Option<String>,
    secret: Option<String>,
}

#[derive(Serialize)]
struct MessageSignData {
    signature: String,
    public_key: String,
    message: String,
}

#[derive(Deserialize, Debug)]
struct MessageVerifyRequest {
    message: Option<String>,
    signature: Option<String>,
    pubkey: Option<String>,
}

#[derive(Serialize)]
struct MessageVerifyData {
    valid: bool,
    message: String,
    pubkey: String,
}

#[derive(Deserialize, Debug)]
struct SendSolRequest {
    from: Option<String>,
    to: Option<String>,
    lamports: Option<u64>,
}

#[derive(Serialize)]
struct SendSolData {
    program_id: String,
    accounts: Vec<String>,
    instruction_data: String,
}

#[derive(Deserialize, Debug)]
struct SendTokenRequest {
    destination: Option<String>,
    mint: Option<String>,
    owner: Option<String>,
    amount: Option<u64>,
    decimals: Option<u8>,
}

#[derive(Serialize)]
struct TokenAccount {
    pubkey: String,
    #[serde(rename = "isSigner")]
    is_signer: bool,
}

#[derive(Serialize)]
struct SendTokenData {
    program_id: String,
    accounts: Vec<TokenAccount>,
    instruction_data: String,
}

#[derive(Deserialize, Debug)]
struct TokenMintRequest {
    mint: String,
    #[serde(rename = "mintAuthority")]
    mintAuthority: String,
    #[serde(rename = "tokenAccount")]
    token_account: String,
    amount: u64,
    decimals: Option<u8>,
}

#[derive(Serialize)]
struct TokenMintData {
    program_id: String,
    accounts: Vec<Account>,
    instruction_data: String,
}

fn get_rpc_url() -> String {
    std::env::var("SOLANA_RPC_URL")
        .unwrap_or_else(|_| "https://api.devnet.solana.com".to_string())
}

/// Parse a public key from multiple possible formats:
/// - Base58 (standard): "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM"
/// - Hex with 0x prefix: "0x123abc..."
/// - Hex without prefix: "123abc..."
fn parse_pubkey_flexible(input: &str) -> Result<Pubkey, String> {
    let trimmed = input.trim();
    
    // First try standard Base58 parsing
    if let Ok(pubkey) = trimmed.parse::<Pubkey>() {
        return Ok(pubkey);
    }
    
    // Try hex formats
    let hex_str = if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
        &trimmed[2..]
    } else {
        trimmed
    };
    
    // Validate hex string length (32 bytes = 64 hex characters)
    if hex_str.len() != 64 {
        return Err(format!("Invalid key length: expected 64 hex characters or valid Base58, got {}", hex_str.len()));
    }
    
    // Validate hex characters
    if !hex_str.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("Invalid hex characters in public key".to_string());
    }
    
    // Try to decode hex
    match hex::decode(hex_str) {
        Ok(bytes) => {
            if bytes.len() != 32 {
                return Err("Invalid key length: must be 32 bytes".to_string());
            }
            
            let mut pubkey_bytes = [0u8; 32];
            pubkey_bytes.copy_from_slice(&bytes);
            Ok(Pubkey::new_from_array(pubkey_bytes))
        }
        Err(_) => Err("Invalid hex format".to_string()),
    }
}

#[tokio::main]
async fn main() {
    // Load environment variables from .env file
    dotenv::dotenv().ok();
    
    let cors = CorsLayer::new()
        // allow requests from exactly our dev server
        .allow_origin("https://superdev.dhruvdeora.com".parse::<axum::http::HeaderValue>().unwrap())
        // or use Any if you really need wildcard
        // .allow_origin(Any)

        // allow common methods (OPTIONS is implicit for pre-flight)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])

        // allow these request headers
        .allow_headers([axum::http::header::CONTENT_TYPE]);

    let api = Router::new()
        .route("/hello", get(hello))
        .route("/airdrop", get(get_airdrop))
        .route("/balance", get(get_balance_query).post(post_balance))
        .route("/keypair", post(get_keypair))
        .route("/token/create", post(create_token))
        .route("/token/mint", post(mint_token))
        .route("/message/sign", post(sign_message))
        .route("/message/verify", post(verify_message))
        .route("/send/sol", post(send_sol))
        .route("/send/token", post(send_token));

    let static_files = || ServeDir::new("../dist")
        .append_index_html_on_directories(true);

    let app = Router::new()
        .nest("/", api)
        // .nest_service("/", static_files())
        .layer(cors);

    let addr: SocketAddr = ([0, 0, 0, 0], 8001).into();
    println!("Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn hello() -> Result<ResponseJson<ApiResponse<HelloData>>, (StatusCode, ResponseJson<ApiResponse<()>>)> {
    println!("GET /hello");
    Ok(ResponseJson(ApiResponse::success(HelloData {
        message: "Hello from Axum!".to_string(),
    })))
}

async fn get_airdrop() -> Result<ResponseJson<ApiResponse<AirdropData>>, (StatusCode, ResponseJson<ApiResponse<()>>)> {
    println!("GET /airdrop");
    let keypair = Keypair::new();
    let pubkey = keypair.pubkey();
    
    let client = RpcClient::new_with_commitment(
        get_rpc_url(),
        CommitmentConfig::confirmed(),
    );
    
    let airdrop_amount = LAMPORTS_PER_SOL;
    
    match client.request_airdrop(&pubkey, airdrop_amount).await {
        Ok(signature) => {
            Ok(ResponseJson(ApiResponse::success(AirdropData {
                account_id: pubkey.to_string(),
                airdrop_signature: signature.to_string(),
            })))
        }
        Err(e) => {
            Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error(format!("Airdrop failed: {}", e)))
            ))
        }
    }
}

async fn get_balance_query(Query(params): Query<BalanceQuery>) -> Result<ResponseJson<ApiResponse<BalanceData>>, (StatusCode, ResponseJson<ApiResponse<()>>)> {
    println!("GET /balance params: {:?}", params);
    let client = RpcClient::new(get_rpc_url());
    
    if params.public_key.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            ResponseJson(ApiResponse::error("Missing required fields".to_string()))
        ));
    }
    
    let pubkey = match parse_pubkey_flexible(&params.public_key) {
        Ok(pk) => pk,
        Err(e) => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error(format!("Invalid public key: {}", e)))
            ));
        }
    };

    match client.get_balance(&pubkey).await {
        Ok(balance) => {
            Ok(ResponseJson(ApiResponse::success(BalanceData { 
                balance,
                public_key: params.public_key,
            })))
        }
        Err(e) => {
            Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error(format!("Failed to get balance: {}", e)))
            ))
        }
    }
}

async fn post_balance(Json(payload): Json<BalanceRequest>) -> Result<ResponseJson<ApiResponse<BalanceData>>, (StatusCode, ResponseJson<ApiResponse<()>>)> {
    println!("POST /balance payload: {:?}", payload);
    let client = RpcClient::new(get_rpc_url());
    
    if payload.public_key.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            ResponseJson(ApiResponse::error("Missing required fields".to_string()))
        ));
    }
    
    let pubkey = match parse_pubkey_flexible(&payload.public_key) {
        Ok(pk) => pk,
        Err(e) => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error(format!("Invalid public key: {}", e)))
            ));
        }
    };

    match client.get_balance(&pubkey).await {
        Ok(balance) => {
            Ok(ResponseJson(ApiResponse::success(BalanceData { 
                balance,
                public_key: payload.public_key,
            })))
        }
        Err(e) => {
            Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error(format!("Failed to get balance: {}", e)))
            ))
        }
    }
}

async fn get_keypair() -> Result<ResponseJson<ApiResponse<KeypairData>>, (StatusCode, ResponseJson<ApiResponse<()>>)> {
    println!("POST /keypair");
    let keypair = Keypair::new();
    let address = keypair.pubkey();
    
    let pubkey = address.to_string();
    
    let secret_bytes = keypair.to_bytes();
    let secret = bs58::encode(&secret_bytes).into_string();
    
    Ok(ResponseJson(ApiResponse::success(KeypairData {
        pubkey,
        secret,
    })))
}

async fn create_token(Json(payload): Json<TokenCreateRequest>) -> Result<ResponseJson<ApiResponse<TokenCreateData>>, (StatusCode, ResponseJson<ApiResponse<()>>)> {
    println!("POST /token/create payload: {:?}", payload);
    // Validate that required fields are not empty
    if payload.mintAuthority.is_empty() || payload.mint.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            ResponseJson(ApiResponse::error("Missing required fields".to_string()))
        ));
    }

    // Validate decimals range (SPL tokens typically use 0-9 decimals)
    if payload.decimals > 9 {
        return Err((
            StatusCode::BAD_REQUEST,
            ResponseJson(ApiResponse::error("Invalid decimals: must be between 0 and 9".to_string()))
        ));
    }

    // Parse the mint authority and mint pubkeys using flexible format support
    let mintAuthority = match parse_pubkey_flexible(&payload.mintAuthority) {
        Ok(pk) => pk,
        Err(e) => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error(format!("Invalid mint authority public key: {}", e)))
            ));
        }
    };

    let mint_pubkey = match parse_pubkey_flexible(&payload.mint) {
        Ok(pk) => pk,
        Err(e) => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error(format!("Invalid mint public key: {}", e)))
            ));
        }
    };

    let initialize_mint_ix = match initialize_mint2(
        &TOKEN_PROGRAM_ID,
        &mint_pubkey,
        &mintAuthority,
        Some(&mintAuthority), 
        payload.decimals,
    ) {
        Ok(ix) => ix,
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                ResponseJson(ApiResponse::error(format!("Failed to create mint instruction: {}", e)))
            ));
        }
    };

    // Extract accounts from the instruction
    let accounts: Vec<Account> = initialize_mint_ix.accounts.iter().map(|acc| {
        Account {
            pubkey: acc.pubkey.to_string(),
            is_signer: acc.is_signer,
            is_writable: acc.is_writable,
        }
    }).collect();

    let instruction_data = base64::encode(&initialize_mint_ix.data);

    let response_data = TokenCreateData {
        program_id: TOKEN_PROGRAM_ID.to_string(),
        accounts,
        instruction_data,
    };

    Ok(ResponseJson(ApiResponse::success(response_data)))
}

async fn mint_token(Json(payload): Json<TokenMintRequest>) -> Result<ResponseJson<ApiResponse<TokenMintData>>, (StatusCode, ResponseJson<ApiResponse<()>>)> {
    println!("POST /token/mint payload: {:?}", payload);
    if payload.mint.is_empty() || payload.mintAuthority.is_empty() || payload.token_account.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            ResponseJson(ApiResponse::error("Missing required fields".to_string()))
        ));
    }

    if payload.amount == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            ResponseJson(ApiResponse::error("Invalid amount: must be greater than 0".to_string()))
        ));
    }

    let decimals = payload.decimals.unwrap_or(9);

    if decimals > 9 {
        return Err((
            StatusCode::BAD_REQUEST,
            ResponseJson(ApiResponse::error("Invalid decimals: must be between 0 and 9".to_string()))
        ));
    }

    let mint_pubkey = match parse_pubkey_flexible(&payload.mint) {
        Ok(pk) => pk,
        Err(e) => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error(format!("Invalid mint public key: {}", e)))
            ));
        }
    };

    let mintAuthority_pubkey = match parse_pubkey_flexible(&payload.mintAuthority) {
        Ok(pk) => pk,
        Err(e) => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error(format!("Invalid mint authority public key: {}", e)))
            ));
        }
    };

    let token_account_pubkey = match parse_pubkey_flexible(&payload.token_account) {
        Ok(pk) => pk,
        Err(e) => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error(format!("Invalid token account public key: {}", e)))
            ));
        }
    };

    let mint_to_ix = match spl_token::instruction::mint_to(
        &TOKEN_PROGRAM_ID,
        &mint_pubkey,
        &token_account_pubkey,
        &mintAuthority_pubkey,
        &[&mintAuthority_pubkey],
        payload.amount,
    ) {
        Ok(ix) => ix,
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                ResponseJson(ApiResponse::error(format!("Failed to create mint instruction: {}", e)))
            ));
        }
    };

    let accounts: Vec<Account> = mint_to_ix.accounts.iter().map(|acc| {
        Account {
            pubkey: acc.pubkey.to_string(),
            is_signer: acc.is_signer,
            is_writable: acc.is_writable,
        }
    }).collect();

    let instruction_data = base64::encode(&mint_to_ix.data);

    let response_data = TokenMintData {
        program_id: TOKEN_PROGRAM_ID.to_string(),
        accounts,
        instruction_data,
    };

    Ok(ResponseJson(ApiResponse::success(response_data)))
}

async fn sign_message(Json(payload): Json<MessageSignRequest>) -> Result<ResponseJson<ApiResponse<MessageSignData>>, (StatusCode, ResponseJson<ApiResponse<()>>)> {
    println!("POST /message/sign payload: {:?}", payload);
    let message = match &payload.message {
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required fields".to_string()))
            ));
        }
        Some(msg) if msg.is_empty() => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required fields".to_string()))
            ));
        }
        Some(msg) => msg,
    };

    let secret = match &payload.secret {
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required fields".to_string()))
            ));
        }
        Some(sec) if sec.is_empty() => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required fields".to_string()))
            ));
        }
        Some(sec) => sec,
    };

    let secret_bytes = match bs58::decode(secret).into_vec() {
        Ok(bytes) => bytes,
        Err(_) => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Invalid secret key format".to_string()))
            ));
        }
    };

    let keypair = match Keypair::from_bytes(&secret_bytes) {
        Ok(kp) => kp,
        Err(_) => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Invalid secret key".to_string()))
            ));
        }
    };

    let signature = keypair.sign_message(message.as_bytes());
    
    let signature_base64 = base64::encode(&signature.as_ref());
    
    let public_key = keypair.pubkey().to_string();

    let response_data = MessageSignData {
        signature: signature_base64,
        public_key,
        message: message.clone(),
    };

    Ok(ResponseJson(ApiResponse::success(response_data)))
}

async fn verify_message(Json(payload): Json<MessageVerifyRequest>) -> Result<ResponseJson<ApiResponse<MessageVerifyData>>, (StatusCode, ResponseJson<ApiResponse<()>>)> {
    println!("POST /message/verify payload: {:?}", payload);
    let message = match &payload.message {
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required fields".to_string()))
            ));
        }
        Some(msg) if msg.is_empty() => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required fields".to_string()))
            ));
        }
        Some(msg) => msg,
    };

    let signature_str = match &payload.signature {
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required fields".to_string()))
            ));
        }
        Some(sig) if sig.is_empty() => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required fields".to_string()))
            ));
        }
        Some(sig) => sig,
    };

    let pubkey_str = match &payload.pubkey {
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required fields".to_string()))
            ));
        }
        Some(pk) if pk.is_empty() => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required fields".to_string()))
            ));
        }
        Some(pk) => pk,
    };

    let signature_bytes = match base64::decode(signature_str) {
        Ok(bytes) => bytes,
        Err(_) => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Invalid signature format".to_string()))
            ));
        }
    };

    let pubkey = match parse_pubkey_flexible(pubkey_str) {
        Ok(pk) => pk,
        Err(e) => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error(format!("Invalid public key: {}", e)))
            ));
        }
    };

    let signature = match Signature::try_from(signature_bytes.as_slice()) {
        Ok(sig) => sig,
        Err(_) => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Invalid signature".to_string()))
            ));
        }
    };

    let is_valid = signature.verify(&pubkey.to_bytes(), message.as_bytes());

    let response_data = MessageVerifyData {
        valid: is_valid,
        message: message.clone(),
        pubkey: pubkey_str.clone(),
    };

    Ok(ResponseJson(ApiResponse::success(response_data)))
}

async fn send_sol(Json(payload): Json<SendSolRequest>) -> Result<ResponseJson<ApiResponse<SendSolData>>, (StatusCode, ResponseJson<ApiResponse<()>>)> {
    println!("POST /send/sol payload: {:?}", payload);
    let from_str = match &payload.from {
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required fields".to_string()))
            ));
        }
        Some(addr) if addr.is_empty() => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required fields".to_string()))
            ));
        }
        Some(addr) => addr,
    };

    let to_str = match &payload.to {
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required fields".to_string()))
            ));
        }
        Some(addr) if addr.is_empty() => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required fields".to_string()))
            ));
        }
        Some(addr) => addr,
    };

    let lamports = match payload.lamports {
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required fields".to_string()))
            ));
        }
        Some(amount) if amount == 0 => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Invalid lamports amount: must be greater than 0".to_string()))
            ));
        }
        Some(amount) if amount > u64::MAX / 2 => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Invalid lamports amount: amount too large".to_string()))
            ));
        }
        Some(amount) => amount,
    };

    let from_pubkey = match parse_pubkey_flexible(from_str) {
        Ok(pk) => pk,
        Err(e) => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error(format!("Invalid 'from' public key: {}", e)))
            ));
        }
    };

    let to_pubkey = match parse_pubkey_flexible(to_str) {
        Ok(pk) => pk,
        Err(e) => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error(format!("Invalid 'to' public key: {}", e)))
            ));
        }
    };


    if from_pubkey == to_pubkey {
        return Err((
            StatusCode::BAD_REQUEST,
            ResponseJson(ApiResponse::error("Cannot send SOL to the same address".to_string()))
        ));
    }
    
    let zero_pubkey = Pubkey::new_from_array([0u8; 32]);
    if from_pubkey == zero_pubkey {
        return Err((
            StatusCode::BAD_REQUEST,
            ResponseJson(ApiResponse::error("Cannot send from zero address".to_string()))
        ));
    }
    
    if to_pubkey == zero_pubkey {
        return Err((
            StatusCode::BAD_REQUEST,
            ResponseJson(ApiResponse::error("Cannot send to zero address".to_string()))
        ));
    }
    
    if to_pubkey == system_program::ID {
        return Err((
            StatusCode::BAD_REQUEST,
            ResponseJson(ApiResponse::error("Cannot send SOL to system program address".to_string()))
        ));
    }
    
    const MAX_REASONABLE_LAMPORTS: u64 = 1_000_000_000 * LAMPORTS_PER_SOL; // 1 billion SOL
    if lamports > MAX_REASONABLE_LAMPORTS {
        return Err((
            StatusCode::BAD_REQUEST,
            ResponseJson(ApiResponse::error("Lamports amount exceeds reasonable limits".to_string()))
        ));
    }

    let transfer_ix: Instruction = system_instruction::transfer(&from_pubkey, &to_pubkey, lamports);

    let accounts: Vec<String> = transfer_ix.accounts.iter().map(|acc| {
        acc.pubkey.to_string()
    }).collect();

    let instruction_data = base64::encode(&transfer_ix.data);

    let response_data = SendSolData {
        program_id: system_program::ID.to_string(),
        accounts,
        instruction_data,
    };

    Ok(ResponseJson(ApiResponse::success(response_data)))
}

async fn send_token(Json(payload): Json<SendTokenRequest>) -> Result<ResponseJson<ApiResponse<SendTokenData>>, (StatusCode, ResponseJson<ApiResponse<()>>)> {
    println!("POST /send/token payload: {:?}", payload);
    let destination_str = match &payload.destination {
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required field: destination".to_string()))
            ));
        }
        Some(addr) if addr.is_empty() => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Invalid destination: cannot be empty".to_string()))
            ));
        }
        Some(addr) => addr,
    };

    let mint_str = match &payload.mint {
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required field: mint".to_string()))
            ));
        }
        Some(addr) if addr.is_empty() => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Invalid mint: cannot be empty".to_string()))
            ));
        }
        Some(addr) => addr,
    };

    let owner_str = match &payload.owner {
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required field: owner".to_string()))
            ));
        }
        Some(addr) if addr.is_empty() => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Invalid owner: cannot be empty".to_string()))
            ));
        }
        Some(addr) => addr,
    };

    let amount = match payload.amount {
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required field: amount".to_string()))
            ));
        }
        Some(amt) if amt == 0 => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Invalid amount: must be greater than 0".to_string()))
            ));
        }
        Some(amt) if amt > u64::MAX / 2 => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Invalid amount: amount too large".to_string()))
            ));
        }
        Some(amt) => amt,
    };

    // Use provided decimals or default to 9 (most common for SPL tokens)
    let decimals = payload.decimals.unwrap_or(9);

    // Parse public keys using flexible format support
    let destination_pubkey = match parse_pubkey_flexible(destination_str) {
        Ok(pk) => pk,
        Err(e) => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error(format!("Invalid 'destination' public key: {}", e)))
            ));
        }
    };

    let mint_pubkey = match parse_pubkey_flexible(mint_str) {
        Ok(pk) => pk,
        Err(e) => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error(format!("Invalid 'mint' public key: {}", e)))
            ));
        }
    };

    let owner_pubkey = match parse_pubkey_flexible(owner_str) {
        Ok(pk) => pk,
        Err(e) => {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error(format!("Invalid 'owner' public key: {}", e)))
            ));
        }
    };

    let source_token_account = get_associated_token_address(&owner_pubkey, &mint_pubkey);
    let destination_token_account = get_associated_token_address(&destination_pubkey, &mint_pubkey);

    if source_token_account == destination_token_account {
        return Err((
            StatusCode::BAD_REQUEST,
            ResponseJson(ApiResponse::error("Cannot transfer tokens to the same account".to_string()))
        ));
    }

    let transfer_ix = match transfer_checked(
        &TOKEN_PROGRAM_ID,
        &source_token_account,
        &mint_pubkey,
        &destination_token_account,
        &owner_pubkey,
        &[&owner_pubkey],
        amount,
        decimals,
    ) {
        Ok(ix) => ix,
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                ResponseJson(ApiResponse::error(format!("Failed to create transfer instruction: {}", e)))
            ));
        }
    };

    let accounts: Vec<TokenAccount> = transfer_ix.accounts.iter().map(|acc| {
        TokenAccount {
            pubkey: acc.pubkey.to_string(),
            is_signer: acc.is_signer,
        }
    }).collect();

    let instruction_data = base64::encode(&transfer_ix.data);

    let response_data = SendTokenData {
        program_id: TOKEN_PROGRAM_ID.to_string(),
        accounts,
        instruction_data,
    };

    Ok(ResponseJson(ApiResponse::success(response_data)))
}

