//! gRPC transport for semstrait using tonic.
//!
//! Exposes `SemstraitGrpcService` implementing the `SemstraitService` proto.
//! Enable with `--features grpc`.

use crate::engine::SharedEngine;
use crate::types::{RawFilter, RawOrderBy, RawQueryRequest};

pub mod proto {
    tonic::include_proto!("semstrait.v1");
}

use proto::semstrait_service_server::{SemstraitService, SemstraitServiceServer};
use proto::{
    ExplainResponse, HealthRequest, HealthResponse, QueryRequest, QueryResponse,
    ValidateResponse,
};

/// Convert a proto `QueryRequest` into the internal `RawQueryRequest`.
fn proto_to_raw(req: QueryRequest) -> RawQueryRequest {
    let raw_filters = req
        .raw_filters
        .into_iter()
        .map(|f| RawFilter {
            field: f.field,
            operator: f.operator,
            value: serde_json::from_str(&f.value_json).unwrap_or(serde_json::Value::Null),
        })
        .collect();

    let order_by = req
        .order_by
        .into_iter()
        .map(|o| RawOrderBy {
            field: o.field,
            direction: if o.direction.is_empty() {
                "asc".to_string()
            } else {
                o.direction
            },
        })
        .collect();

    RawQueryRequest {
        model: req.model,
        from: req.from,
        select: req.select,
        filters: req.filters,
        raw_filters,
        grain: req.grain,
        limit: req.limit,
        order_by,
        session: req.session,
        engine: req.engine,
    }
}

/// gRPC service backed by the shared SemstraitEngine.
pub struct SemstraitGrpcService {
    engine: SharedEngine,
}

impl SemstraitGrpcService {
    pub fn new(engine: SharedEngine) -> Self {
        Self { engine }
    }

    /// Create a tonic `Router`-compatible server from this service.
    pub fn into_server(self) -> SemstraitServiceServer<Self> {
        SemstraitServiceServer::new(self)
    }
}

#[tonic::async_trait]
impl SemstraitService for SemstraitGrpcService {
    async fn health(
        &self,
        _request: tonic::Request<HealthRequest>,
    ) -> Result<tonic::Response<HealthResponse>, tonic::Status> {
        Ok(tonic::Response::new(HealthResponse {
            status: "ok".to_string(),
        }))
    }

    async fn explain(
        &self,
        request: tonic::Request<QueryRequest>,
    ) -> Result<tonic::Response<ExplainResponse>, tonic::Status> {
        let raw = proto_to_raw(request.into_inner());

        let result = self
            .engine
            .explain(&raw)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        Ok(tonic::Response::new(ExplainResponse {
            plan_text: result.plan_text,
            sql: result.sql,
            substrait_json: result.substrait_json,
        }))
    }

    async fn validate(
        &self,
        request: tonic::Request<QueryRequest>,
    ) -> Result<tonic::Response<ValidateResponse>, tonic::Status> {
        let raw = proto_to_raw(request.into_inner());

        let result = self.engine.validate(&raw);

        Ok(tonic::Response::new(ValidateResponse {
            valid: result.valid,
            errors: result.errors,
            warnings: result.warnings,
        }))
    }

    async fn query(
        &self,
        request: tonic::Request<QueryRequest>,
    ) -> Result<tonic::Response<QueryResponse>, tonic::Status> {
        let raw = proto_to_raw(request.into_inner());

        let result = self
            .engine
            .query(&raw)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        // Serialize each row to a JSON string.
        let rows_json: Vec<String> = match result.as_array() {
            Some(arr) => arr
                .iter()
                .map(|v| serde_json::to_string(v).unwrap_or_default())
                .collect(),
            None => vec![serde_json::to_string(&result).unwrap_or_default()],
        };
        let rows_returned = rows_json.len() as u64;

        Ok(tonic::Response::new(QueryResponse {
            rows_json,
            rows_returned,
        }))
    }
}
