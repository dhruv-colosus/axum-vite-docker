use axum::{
    routing::{get, post},
    Router,
    http::Method,
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
};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct AirdropResponse {
    account_id: String,
    airdrop_signature: String,
}

#[derive(Serialize)]
struct BalanceResponse {
    balance: u64,
    public_key: String,
}

#[derive(Deserialize)]
struct BalanceRequest {
    public_key: String,
}

#[derive(Deserialize)]
struct BalanceQuery {
    public_key: String,
}

fn get_rpc_url() -> String {
    std::env::var("SOLANA_RPC_URL")
        .unwrap_or_else(|_| "https://api.devnet.solana.com".to_string())
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
    // ➊ API sub-router
    let api = Router::new()
        .route("/hello", get(|| async { "Hello from Axum!" }))
        .route("/airdrop", get(get_airdrop))
        .route("/balance", get(get_balance_query).post(post_balance));

    // ➋ Static files (relative to *binary working dir*)
    let static_files = || ServeDir::new("../dist")
        // Axum 0.7: add `append_index_html_on_directories` if you want "about" to load /about/index.html
        .append_index_html_on_directories(true);

    // ➌ Compose:
    //     /api/...   -> handled by API router
    //     everything else -> static build (e.g. /, /about, /assets/...)
    let app = Router::new()
        .nest("/api", api)
        .nest_service("/", static_files())
        // .fallback_service(static_files())
        .layer(cors);          

    let addr: SocketAddr = ([0, 0, 0, 0], 8001).into();
    println!("Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn get_airdrop() -> ResponseJson<AirdropResponse> {
    // Create a new keypair
    let keypair = Keypair::new();
    let pubkey = keypair.pubkey();
    
    // Create RPC client with commitment config (like the working example)
    let client = RpcClient::new_with_commitment(
        get_rpc_url(),
        CommitmentConfig::confirmed(),
    );
    
    // Request airdrop (1 SOL = 1_000_000_000 lamports)
    let airdrop_amount = LAMPORTS_PER_SOL; // 1 SOL
    
    match client.request_airdrop(&pubkey, airdrop_amount).await {
        Ok(signature) => {
            ResponseJson(AirdropResponse {
                account_id: pubkey.to_string(),
                airdrop_signature: signature.to_string(),
            })
        }
        Err(_) => {
            // Return a default response on error
            ResponseJson(AirdropResponse {
                account_id: pubkey.to_string(),
                airdrop_signature: "Error: Airdrop failed".to_string(),
            })
        }
    }
}

async fn get_balance_query(Query(params): Query<BalanceQuery>) -> ResponseJson<BalanceResponse> {
    let client = RpcClient::new(get_rpc_url());
    println!("rpc url: {}", get_rpc_url());
    
    match params.public_key.parse::<Pubkey>() {
        Ok(pubkey) => {
            match client.get_balance(&pubkey).await {
                Ok(balance) => ResponseJson(BalanceResponse { 
                    balance,
                    public_key: params.public_key,
                }),
                Err(_) => ResponseJson(BalanceResponse { 
                    balance: 0,
                    public_key: params.public_key,
                }),
            }
        }
        Err(_) => ResponseJson(BalanceResponse { 
            balance: 0,
            public_key: params.public_key,
        }),
    }
}

async fn post_balance(Json(payload): Json<BalanceRequest>) -> ResponseJson<BalanceResponse> {
    let client = RpcClient::new(get_rpc_url());
    
    match payload.public_key.parse::<Pubkey>() {
        Ok(pubkey) => {
            match client.get_balance(&pubkey).await {
                Ok(balance) => ResponseJson(BalanceResponse { 
                    balance,
                    public_key: payload.public_key,
                }),
                Err(_) => ResponseJson(BalanceResponse { 
                    balance: 0,
                    public_key: payload.public_key,
                }),
            }
        }
        Err(_) => ResponseJson(BalanceResponse { 
            balance: 0,
            public_key: payload.public_key,
        }),
    }
}

async fn get_balance() -> ResponseJson<BalanceResponse> {
    let client = RpcClient::new(get_rpc_url());
    let pubkey: Pubkey = "Cz2HjU8B3uWPhamkQhRpbHqM19iYby6ZQujYBK4rFVHX".parse().unwrap();
    let balance = client.get_balance(&pubkey).await.unwrap();
    ResponseJson(BalanceResponse { 
        balance,
        public_key: pubkey.to_string(),
    })
}

