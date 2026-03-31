//! REST API transport using axum.
//!
//! Routes:
//! - POST /query    → execute query
//! - POST /explain  → explain plan + SQL
//! - POST /validate → validate request
//! - POST /compile  → compile YAML model to manifest JSON
//! - GET  /schema   → introspect manifest kinds/dimensions/measures
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
use semstrait_manifest::{CompileSource, ManifestCompiler};

/// Create the REST API router.
pub fn router(engine: SharedEngine) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/schema", get(schema))
        .route("/validate", post(validate))
        .route("/explain", post(explain))
        .route("/query", post(query))
        .route("/compile", post(compile))
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

async fn schema(State(engine): State<SharedEngine>) -> impl IntoResponse {
    match engine.manifest() {
        Some(manifest) => {
            let mut kinds = serde_json::Map::new();
            for (name, data_kind) in &manifest.data_kinds {
                let iface = data_kind.interface();
                let dims: Vec<&str> = iface.dimensions.keys().map(|s| s.as_str()).collect();
                let measures: Vec<&str> = iface.measures.keys().map(|s| s.as_str()).collect();
                let metrics: Vec<&str> = iface.metrics.keys().map(|s| s.as_str()).collect();
                kinds.insert(
                    name.clone(),
                    serde_json::json!({
                        "dimensions": dims,
                        "measures": measures,
                        "metrics": metrics,
                    }),
                );
            }
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "model_name": manifest.model_name,
                    "data_kinds": kinds,
                })),
            )
        }
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "no manifest loaded" })),
        ),
    }
}

async fn compile(body: String) -> impl IntoResponse {
    let compiler = ManifestCompiler::new();
    match compiler.compile(CompileSource::Yaml(body)).await {
        Ok(manifest) => match serde_json::to_value(&manifest) {
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
