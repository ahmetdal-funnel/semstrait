//! Canonical DataType ↔ Substrait proto::Type mapping.
//!
//! Fixed 1:1 mapping between the semantic model's canonical types and Substrait
//! type representations. Engine-specific type decisions happen at plan construction
//! (via PlanBuilder), not at serialization time.

use semstrait_core::DataType;
use substrait::proto::{
    self,
    r#type::{Kind, Nullability},
};

/// Map a canonical `DataType` to its Substrait `proto::Type` representation.
pub(crate) fn datatype_to_substrait(dt: &DataType) -> proto::Type {
    let kind = match dt {
        DataType::Integer => Kind::I64(proto::r#type::I64 {
            type_variation_reference: 0,
            nullability: Nullability::Nullable as i32,
        }),
        DataType::Number => Kind::Fp64(proto::r#type::Fp64 {
            type_variation_reference: 0,
            nullability: Nullability::Nullable as i32,
        }),
        DataType::Boolean => Kind::Bool(proto::r#type::Boolean {
            type_variation_reference: 0,
            nullability: Nullability::Nullable as i32,
        }),
        DataType::String => Kind::String(proto::r#type::String {
            type_variation_reference: 0,
            nullability: Nullability::Nullable as i32,
        }),
        DataType::Date => Kind::Date(proto::r#type::Date {
            type_variation_reference: 0,
            nullability: Nullability::Nullable as i32,
        }),
        DataType::Timestamp { precision } => {
            Kind::PrecisionTimestamp(proto::r#type::PrecisionTimestamp {
                precision: *precision as i32,
                type_variation_reference: 0,
                nullability: Nullability::Nullable as i32,
            })
        }
        DataType::Decimal { precision, scale } => {
            Kind::Decimal(proto::r#type::Decimal {
                precision: *precision as i32,
                scale: *scale as i32,
                type_variation_reference: 0,
                nullability: Nullability::Nullable as i32,
            })
        }
        DataType::Binary => Kind::Binary(proto::r#type::Binary {
            type_variation_reference: 0,
            nullability: Nullability::Nullable as i32,
        }),
    };

    proto::Type { kind: Some(kind) }
}

/// Map a Substrait `proto::Type` back to a canonical `DataType`.
#[allow(deprecated)]
pub(crate) fn substrait_to_datatype(typ: &proto::Type) -> DataType {
    match &typ.kind {
        Some(Kind::I8(_) | Kind::I16(_) | Kind::I32(_) | Kind::I64(_)) => DataType::Integer,
        Some(Kind::Fp32(_) | Kind::Fp64(_)) => DataType::Number,
        Some(Kind::Bool(_)) => DataType::Boolean,
        Some(Kind::String(_)) => DataType::String,
        Some(Kind::Date(_)) => DataType::Date,
        Some(Kind::PrecisionTimestamp(pt)) => DataType::Timestamp {
            precision: pt.precision as u8,
        },
        Some(Kind::Timestamp(_)) => DataType::Timestamp { precision: 6 },
        Some(Kind::Decimal(d)) => DataType::Decimal {
            precision: d.precision as u8,
            scale: d.scale as i8,
        },
        Some(Kind::Binary(_)) => DataType::Binary,
        _ => DataType::String,
    }
}
