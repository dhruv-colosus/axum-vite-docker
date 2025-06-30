use axum::{
    routing::get,
    Router,
    http::Method,
};
use tower_http::{
    services::ServeDir,
    cors::{CorsLayer},
};
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
      let cors = CorsLayer::new()
        // allow requests from exactly our dev server
        .allow_origin("http://localhost:5173".parse::<axum::http::HeaderValue>().unwrap())
        // or use Any if you really need wildcard
        // .allow_origin(Any)

        // allow common methods (OPTIONS is implicit for pre-flight)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])

        // allow these request headers
        .allow_headers([axum::http::header::CONTENT_TYPE]);
    // ➊ API sub-router
    let api = Router::new()
        .route("/hello", get(|| async { "Hello from Axum!" }));

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

