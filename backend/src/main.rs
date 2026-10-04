mod calc;
mod db;
mod handlers;
mod mockpay;
mod models;
mod seed;

use axum::{
    routing::{get, post},
    Router,
};
use tower_http::{cors::CorsLayer, services::ServeDir};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        )
        .init();

    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://data/app.db".into());
    let pool = db::init(&db_url).await;
    seed::seed_if_empty(&pool).await;
    tracing::info!("数据库就绪：{}", db_url);

    let api = Router::new()
        // 顾客端
        .route("/orders", get(handlers::list_orders))
        .route("/price-protection/preview", get(handlers::preview))
        .route("/applications", get(handlers::my_applications).post(handlers::apply))
        // 店员端
        .route("/staff/applications", get(handlers::staff_queue))
        .route("/staff/applications/{id}/approve", post(handlers::approve))
        .route("/staff/applications/{id}/reject", post(handlers::reject))
        .route("/staff/refunds", get(handlers::list_refunds))
        // 支付回执 webhook（幂等）
        .route("/webhooks/payment", post(handlers::payment_webhook))
        // 元数据与演示控制
        .route("/meta", get(handlers::meta))
        .route("/demo/price-drop", post(handlers::price_drop))
        .route("/demo/receipts/{receipt_no}/resend", post(handlers::resend_receipt))
        .route("/demo/reset", post(handlers::reset_demo))
        .route("/mock-pay/refunds", get(handlers::mock_pay_ledger))
        .layer(CorsLayer::permissive())
        .with_state(pool);

    // 前端静态文件（frontend/dist），所有非 /api 路径回退到 index.html
    let dist = std::env::var("FRONTEND_DIST").unwrap_or_else(|_| "../frontend/dist".into());
    let app = Router::new()
        .nest("/api", api)
        .fallback_service(ServeDir::new(&dist).append_index_html_on_directories(true));

    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".into());
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind");
    tracing::info!("服务已启动：http://localhost:3000 （前端静态目录：{}）", dist);
    axum::serve(listener, app).await.unwrap();
}
