//! REST API transport using axum.
//!
//! Routes:
//! - POST /query    → execute query (v2)
//! - POST /explain  → explain plan + SQL
//! - POST /validate → validate request
//! - GET  /health   → health check

use crate::engine::SharedEngine;
use crate::types::RawQueryRequest;
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};

/// Create the REST API router.
pub fn router(engine: SharedEngine) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/validate", post(validate))
        .route("/explain", post(explain))
        .route("/query", post(query))
        .with_state(engine)
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn validate(
    State(engine): State<SharedEngine>,
    Json(raw): Json<RawQueryRequest>,
) -> impl IntoResponse {
    let result = engine.validate(&raw);
    let status = if result.valid {
        StatusCode::OK
    } else {
        StatusCode::BAD_REQUEST
    };
    (status, Json(result))
}

async fn explain(
    State(engine): State<SharedEngine>,
    Json(raw): Json<RawQueryRequest>,
) -> impl IntoResponse {
    match engine.explain(&raw).await {
        Ok(result) => match serde_json::to_value(result) {
            Ok(v) => (StatusCode::OK, Json(v)),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("serialization error: {}", e) })),
            ),
        },
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

async fn query(
    State(engine): State<SharedEngine>,
    Json(raw): Json<RawQueryRequest>,
) -> impl IntoResponse {
    match engine.query(&raw).await {
        Ok(result) => (StatusCode::OK, Json(result)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}
