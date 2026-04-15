//! Expr <-> Substrait Expression converter

use crate::error::ConvertError;
use crate::rewrite::CanonicalFn;
use crate::schema::Schema;
use super::anchors::*;

use semstrait_core::DataType;
use semstrait_core::expr::{Aggregation, BinaryOp, Expr, Literal, WhenClause};
use substrait::proto::{
    self,
    expression::{
        self, reference_segment::ReferenceType, ReferenceSegment,
    },
    function_argument::ArgType,
};

/// Converts Expr to/from Substrait Expression
pub struct ExprConverter<'s> {
    schema: &'s Schema,
}

impl<'s> ExprConverter<'s> {
    pub fn new(schema: &'s Schema) -> Self {
        Self { schema }
    }

    /// Convert Expr to Substrait Expression
    pub fn to_substrait(&self, expr: &Expr) -> Result<proto::Expression, ConvertError> {
        match expr {
            Expr::Column(col) => {
                let field_name = if let Some(q) = &col.qualifier {
                    format!("{}.{}", q, col.name)
                } else {
                    col.name.clone()
                };

                let ordinal = self
                    .schema
                    .ordinal(&field_name)
                    .or_else(|| self.schema.ordinal(&col.name))
                    .ok_or_else(|| ConvertError::ColumnNotFound(field_name.clone()))?;

                self.field_reference(ordinal as u32)
            }

            Expr::Literal(lit) => match lit {
                Literal::Integer { value } => Ok(literal_i64(*value)),
                Literal::Float { value } => Ok(literal_f64(*value)),
                Literal::String { value } => Ok(literal_string(value)),
                Literal::Boolean { value } => Ok(literal_bool(*value)),
                Literal::Null => Ok(literal_null()),
            },

            Expr::BinaryOp(bin) => {
                let left_expr = self.to_substrait(&bin.left)?;
                let right_expr = self.to_substrait(&bin.right)?;
                self.binary_op(left_expr, bin.op, right_expr)
            }

            Expr::Aggregate(agg) => {
                let inner = self.to_substrait(&agg.expr)?;
                let name = match agg.function {
                    Aggregation::Sum => "sum",
                    Aggregation::Avg => "avg",
                    Aggregation::Count | Aggregation::CountDistinct => "count",
                    Aggregation::Min => "min",
                    Aggregation::Max => "max",
                };
                self.function_call(name, vec![inner])
            }

            Expr::FunctionCall(fc) => {
                let arg_exprs: Result<Vec<_>, _> =
                    fc.args.iter().map(|a| self.to_substrait(a)).collect();
                self.function_call(&fc.name, arg_exprs?)
            }

            Expr::Negate(u) => {
                let inner_expr = self.to_substrait(&u.expr)?;
                let zero = literal_f64(0.0);
                self.binary_op(zero, BinaryOp::Subtract, inner_expr)
            }

            Expr::Case(c) => {
                let ifs: Result<Vec<_>, _> = c
                    .when_then
                    .iter()
                    .map(|wc| {
                        let if_expr = self.to_substrait(&wc.condition)?;
                        let then_expr = self.to_substrait(&wc.result)?;
                        Ok(expression::if_then::IfClause {
                            r#if: Some(if_expr),
                            then: Some(then_expr),
                        })
                    })
                    .collect();

                let else_result = c
                    .else_expr
                    .as_ref()
                    .map(|e| self.to_substrait(e))
                    .transpose()?
                    .map(Box::new);

                Ok(proto::Expression {
                    rex_type: Some(expression::RexType::IfThen(Box::new(
                        expression::IfThen {
                            ifs: ifs?,
                            r#else: else_result,
                        },
                    ))),
                })
            }

            Expr::Not(u) => {
                let inner_expr = self.to_substrait(&u.expr)?;
                self.function_call("not", vec![inner_expr])
            }

            Expr::IsNull(u) => {
                let inner_expr = self.to_substrait(&u.expr)?;
                self.function_call("is_null", vec![inner_expr])
            }

            Expr::IsNotNull(u) => {
                let inner_expr = self.to_substrait(&u.expr)?;
                self.function_call("is_not_null", vec![inner_expr])
            }

            Expr::InList(il) => {
                let value = self.to_substrait(&il.expr)?;
                let options: Result<Vec<_>, _> =
                    il.list.iter().map(|e| self.to_substrait(e)).collect();
                Ok(proto::Expression {
                    rex_type: Some(expression::RexType::SingularOrList(Box::new(
                        expression::SingularOrList {
                            value: Some(Box::new(value)),
                            options: options?,
                        },
                    ))),
                })
            }

            Expr::Between(bt) => {
                let args = vec![
                    self.to_substrait(&bt.expr)?,
                    self.to_substrait(&bt.low)?,
                    self.to_substrait(&bt.high)?,
                ];
                self.function_call("between", args)
            }

            Expr::Like(lk) => {
                let args = vec![
                    self.to_substrait(&lk.expr)?,
                    self.to_substrait(&lk.pattern)?,
                ];
                self.function_call("like", args)
            }

            Expr::Coalesce(co) => {
                let args: Result<Vec<_>, _> =
                    co.exprs.iter().map(|e| self.to_substrait(e)).collect();
                self.function_call("coalesce", args?)
            }

            Expr::NullIf(ni) => {
                let args = vec![
                    self.to_substrait(&ni.expr)?,
                    self.to_substrait(&ni.null_expr)?,
                ];
                self.function_call("nullif", args)
            }

            Expr::DateTrunc(dt) => {
                let grain_expr = literal_string(&dt.grain.to_string());
                let inner_expr = self.to_substrait(&dt.expr)?;
                self.function_call("date_trunc", vec![grain_expr, inner_expr])
            }

            Expr::ILike(lk) => {
                let args = vec![
                    self.to_substrait(&lk.expr)?,
                    self.to_substrait(&lk.pattern)?,
                ];
                self.function_call("ilike", args)
            }

            Expr::RegexpMatch(re) => {
                let args = vec![
                    self.to_substrait(&re.expr)?,
                    self.to_substrait(&re.pattern)?,
                    literal_bool(re.full_match),
                ];
                self.function_call("regexp_match", args)
            }

            Expr::RegexpExtract(re) => {
                let args = vec![
                    self.to_substrait(&re.expr)?,
                    self.to_substrait(&re.pattern)?,
                    literal_i64(re.group_idx as i64),
                ];
                self.function_call("regexp_extract", args)
            }

            Expr::Cast(c) => {
                let inner = self.to_substrait(&c.expr)?;
                let target_type = super::datatype_to_substrait(&c.data_type);
                Ok(proto::Expression {
                    rex_type: Some(expression::RexType::Cast(Box::new(expression::Cast {
                        r#type: Some(target_type),
                        input: Some(Box::new(inner)),
                        failure_behavior: expression::cast::FailureBehavior::ReturnNull as i32,
                    }))),
                })
            }

            Expr::EntityRef(_) => Err(ConvertError::UnsupportedExpression(
                "EntityRef should be resolved before Substrait conversion".to_string(),
            )),

            Expr::Guard(_) => Err(ConvertError::UnsupportedExpression(
                "Guard should be expanded to Case before Substrait conversion".to_string(),
            )),
        }
    }

    /// Convert Substrait Expression to Expr
    pub fn from_substrait(
        &self,
        expr: &proto::Expression,
    ) -> Result<Expr, ConvertError> {
        match &expr.rex_type {
            Some(expression::RexType::Literal(lit)) => from_literal(lit),
            Some(expression::RexType::Selection(field_ref)) => {
                self.from_field_reference(field_ref)
            }
            Some(expression::RexType::ScalarFunction(func)) => {
                self.from_scalar_function(func)
            }
            Some(expression::RexType::IfThen(if_then)) => self.from_if_then(if_then),
            Some(expression::RexType::Cast(cast)) => self.from_cast(cast),
            Some(expression::RexType::SingularOrList(sol)) => self.from_singular_or_list(sol),
            _ => Err(ConvertError::UnsupportedExpression(format!(
                "Unsupported expression type: {:?}",
                expr.rex_type
            ))),
        }
    }

    fn field_reference(&self, field: u32) -> Result<proto::Expression, ConvertError> {
        Ok(proto::Expression {
            rex_type: Some(expression::RexType::Selection(Box::new(
                proto::expression::FieldReference {
                    reference_type: Some(
                        expression::field_reference::ReferenceType::DirectReference(
                            ReferenceSegment {
                                reference_type: Some(ReferenceType::StructField(Box::new(
                                    expression::reference_segment::StructField {
                                        field: field as i32,
                                        child: None,
                                    },
                                ))),
                            },
                        ),
                    ),
                    root_type: None,
                },
            ))),
        })
    }

    fn binary_op(
        &self,
        left: proto::Expression,
        op: BinaryOp,
        right: proto::Expression,
    ) -> Result<proto::Expression, ConvertError> {
        let function_ref = match op {
            BinaryOp::Eq => FUNC_EQUAL,
            BinaryOp::NotEq => FUNC_NOT_EQUAL,
            BinaryOp::Lt => FUNC_LT,
            BinaryOp::LtEq => FUNC_LTE,
            BinaryOp::Gt => FUNC_GT,
            BinaryOp::GtEq => FUNC_GTE,
            BinaryOp::And => FUNC_AND,
            BinaryOp::Or => FUNC_OR,
            BinaryOp::Add => FUNC_ADD,
            BinaryOp::Subtract => FUNC_SUBTRACT,
            BinaryOp::Multiply => FUNC_MULTIPLY,
            BinaryOp::Divide => FUNC_DIVIDE,
            BinaryOp::SafeDivide => FUNC_DIVIDE,
        };

        Ok(scalar_function(function_ref, vec![left, right]))
    }

    fn function_call(
        &self,
        name: &str,
        args: Vec<proto::Expression>,
    ) -> Result<proto::Expression, ConvertError> {
        let function_ref = match name.to_lowercase().as_str() {
            // Dedicated Expr variant functions (anchors in anchors.rs)
            "not" => FUNC_NOT,
            "is_null" => FUNC_IS_NULL,
            "is_not_null" => FUNC_IS_NOT_NULL,
            "in" => FUNC_IN,
            "between" => FUNC_BETWEEN,
            "like" => FUNC_LIKE,
            "ilike" => FUNC_ILIKE,
            "regexp_match" => FUNC_REGEXP_MATCH,
            "regexp_extract" => FUNC_REGEXP_EXTRACT,
            "coalesce" => FUNC_COALESCE,
            "nullif" => FUNC_NULLIF,
            "date_trunc" => FUNC_DATE_TRUNC,
            "cast" => FUNC_CAST,
            "sum" | "avg" | "count" | "min" | "max" => {
                // Aggregate functions mapped to arithmetic URI for now
                FUNC_ADD // placeholder — real aggregate handling is in serializer
            }
            // Canonical functions — anchor via CanonicalFn
            other => {
                if let Some(cf) = CanonicalFn::from_name(other) {
                    cf.anchor()
                } else {
                    return Err(ConvertError::FunctionNotFound(format!(
                        "Function not mapped: {}",
                        name
                    )));
                }
            }
        };

        Ok(scalar_function(function_ref, args))
    }

    #[allow(clippy::wrong_self_convention)]
    fn from_field_reference(
        &self,
        field_ref: &proto::expression::FieldReference,
    ) -> Result<Expr, ConvertError> {
        let ordinal = match &field_ref.reference_type {
            Some(expression::field_reference::ReferenceType::DirectReference(seg)) => {
                match &seg.reference_type {
                    Some(ReferenceType::StructField(sf)) => sf.field as usize,
                    _ => {
                        return Err(ConvertError::UnsupportedExpression(
                            "Unsupported field reference type".to_string(),
                        ))
                    }
                }
            }
            _ => {
                return Err(ConvertError::UnsupportedExpression(
                    "Unsupported field reference".to_string(),
                ))
            }
        };

        let field = self
            .schema
            .field(ordinal)
            .ok_or_else(|| ConvertError::ColumnNotFound(format!("Field at index {}", ordinal)))?;

        Ok(Expr::column(field.name.clone()))
    }

    #[allow(clippy::wrong_self_convention)]
    fn from_scalar_function(
        &self,
        func: &proto::expression::ScalarFunction,
    ) -> Result<Expr, ConvertError> {
        let args: Result<Vec<_>, _> = func
            .arguments
            .iter()
            .filter_map(|arg| match &arg.arg_type {
                Some(ArgType::Value(expr)) => Some(self.from_substrait(expr)),
                _ => None,
            })
            .collect();
        let mut args = args?;

        match func.function_reference {
            FUNC_EQUAL => self.binary_from_args(args, BinaryOp::Eq),
            FUNC_NOT_EQUAL => self.binary_from_args(args, BinaryOp::NotEq),
            FUNC_LT => self.binary_from_args(args, BinaryOp::Lt),
            FUNC_LTE => self.binary_from_args(args, BinaryOp::LtEq),
            FUNC_GT => self.binary_from_args(args, BinaryOp::Gt),
            FUNC_GTE => self.binary_from_args(args, BinaryOp::GtEq),
            FUNC_AND => self.binary_from_args(args, BinaryOp::And),
            FUNC_OR => self.binary_from_args(args, BinaryOp::Or),
            FUNC_ADD => self.binary_from_args(args, BinaryOp::Add),
            FUNC_SUBTRACT => self.binary_from_args(args, BinaryOp::Subtract),
            FUNC_MULTIPLY => self.binary_from_args(args, BinaryOp::Multiply),
            FUNC_DIVIDE => self.binary_from_args(args, BinaryOp::Divide),
            FUNC_NOT => {
                if args.len() != 1 {
                    return Err(ConvertError::InvalidExpression(format!(
                        "NOT requires 1 argument, got {}",
                        args.len()
                    )));
                }
                Ok(Expr::not(args.into_iter().next().unwrap()))
            }
            FUNC_IS_NULL => {
                if args.len() != 1 {
                    return Err(ConvertError::InvalidExpression(format!(
                        "IS_NULL requires 1 argument, got {}",
                        args.len()
                    )));
                }
                Ok(Expr::is_null(args.into_iter().next().unwrap()))
            }
            FUNC_IS_NOT_NULL => {
                if args.len() != 1 {
                    return Err(ConvertError::InvalidExpression(format!(
                        "IS_NOT_NULL requires 1 argument, got {}",
                        args.len()
                    )));
                }
                Ok(Expr::is_not_null(args.into_iter().next().unwrap()))
            }
            FUNC_IN => {
                if args.len() < 2 {
                    return Err(ConvertError::InvalidExpression(format!(
                        "IN requires at least 2 arguments, got {}",
                        args.len()
                    )));
                }
                let expr = args.remove(0);
                Ok(Expr::in_list(expr, args))
            }
            FUNC_BETWEEN => {
                if args.len() != 3 {
                    return Err(ConvertError::InvalidExpression(format!(
                        "BETWEEN requires 3 arguments, got {}",
                        args.len()
                    )));
                }
                let mut iter = args.into_iter();
                let expr = iter.next().unwrap();
                let low = iter.next().unwrap();
                let high = iter.next().unwrap();
                Ok(Expr::between(expr, low, high))
            }
            FUNC_LIKE => {
                if args.len() != 2 {
                    return Err(ConvertError::InvalidExpression(format!(
                        "LIKE requires 2 arguments, got {}",
                        args.len()
                    )));
                }
                let mut iter = args.into_iter();
                let expr = iter.next().unwrap();
                let pattern = iter.next().unwrap();
                Ok(Expr::like(expr, pattern))
            }
            FUNC_ILIKE => {
                if args.len() != 2 {
                    return Err(ConvertError::InvalidExpression(format!(
                        "ILIKE requires 2 arguments, got {}",
                        args.len()
                    )));
                }
                let mut iter = args.into_iter();
                let expr = iter.next().unwrap();
                let pattern = iter.next().unwrap();
                Ok(Expr::ilike(expr, pattern))
            }
            FUNC_REGEXP_MATCH => {
                if args.len() != 3 {
                    return Err(ConvertError::InvalidExpression(format!(
                        "REGEXP_MATCH requires 3 arguments, got {}",
                        args.len()
                    )));
                }
                let mut iter = args.into_iter();
                let expr = iter.next().unwrap();
                let pattern = iter.next().unwrap();
                let full_match = match &iter.next().unwrap() {
                    Expr::Literal(Literal::Boolean { value }) => *value,
                    _ => false,
                };
                Ok(Expr::regexp_match(expr, pattern, full_match))
            }
            FUNC_REGEXP_EXTRACT => {
                if args.len() != 3 {
                    return Err(ConvertError::InvalidExpression(format!(
                        "REGEXP_EXTRACT requires 3 arguments, got {}",
                        args.len()
                    )));
                }
                let mut iter = args.into_iter();
                let expr = iter.next().unwrap();
                let pattern = iter.next().unwrap();
                let group_idx = match &iter.next().unwrap() {
                    Expr::Literal(Literal::Integer { value }) => *value as usize,
                    _ => 0,
                };
                Ok(Expr::regexp_extract(expr, pattern, group_idx))
            }
            FUNC_COALESCE => Ok(Expr::coalesce(args)),
            FUNC_NULLIF => {
                if args.len() != 2 {
                    return Err(ConvertError::InvalidExpression(format!(
                        "NULLIF requires 2 arguments, got {}",
                        args.len()
                    )));
                }
                let mut iter = args.into_iter();
                let expr = iter.next().unwrap();
                let null_expr = iter.next().unwrap();
                Ok(Expr::null_if(expr, null_expr))
            }
            FUNC_DATE_TRUNC => {
                if args.len() != 2 {
                    return Err(ConvertError::InvalidExpression(format!(
                        "DATE_TRUNC requires 2 arguments, got {}",
                        args.len()
                    )));
                }
                let grain = match &args[0] {
                    Expr::Literal(Literal::String { value }) => value.clone(),
                    _ => {
                        return Err(ConvertError::InvalidExpression(
                            "DATE_TRUNC first argument must be a string literal".to_string(),
                        ))
                    }
                };
                let inner = args.swap_remove(1);
                let grain_enum = grain.parse::<semstrait_core::Grain>().map_err(|_| {
                    ConvertError::InvalidExpression(format!("Unknown grain: {}", grain))
                })?;
                Ok(Expr::date_trunc(grain_enum, inner))
            }
            FUNC_CAST => {
                if args.len() != 2 {
                    return Err(ConvertError::InvalidExpression(format!(
                        "CAST requires 2 arguments, got {}",
                        args.len()
                    )));
                }
                let mut iter = args.into_iter();
                let expr = iter.next().unwrap();
                let type_name = match iter.next().unwrap() {
                    Expr::Literal(Literal::String { value }) => value,
                    _ => {
                        return Err(ConvertError::InvalidExpression(
                            "CAST second argument must be a string literal (data type)".to_string(),
                        ))
                    }
                };
                let data_type: DataType = type_name.parse().map_err(|e: String| {
                    ConvertError::InvalidExpression(format!("invalid cast type '{}': {}", type_name, e))
                })?;
                Ok(Expr::cast(expr, data_type))
            }
            _ => Err(ConvertError::FunctionNotFound(format!(
                "Unknown function reference: {}",
                func.function_reference
            ))),
        }
    }

    fn binary_from_args(
        &self,
        mut args: Vec<Expr>,
        op: BinaryOp,
    ) -> Result<Expr, ConvertError> {
        if args.len() != 2 {
            return Err(ConvertError::InvalidExpression(format!(
                "Binary operator requires 2 arguments, got {}",
                args.len()
            )));
        }
        let right = args.pop().ok_or_else(|| {
            ConvertError::InvalidExpression("missing right operand".into())
        })?;
        let left = args.pop().ok_or_else(|| {
            ConvertError::InvalidExpression("missing left operand".into())
        })?;
        Ok(Expr::binary(left, op, right))
    }

    #[allow(clippy::wrong_self_convention)]
    fn from_if_then(
        &self,
        if_then: &expression::IfThen,
    ) -> Result<Expr, ConvertError> {
        let when_then: Result<Vec<_>, _> = if_then
            .ifs
            .iter()
            .map(|clause| {
                let cond = clause
                    .r#if
                    .as_ref()
                    .ok_or_else(|| ConvertError::MissingField("if condition".to_string()))?;
                let result = clause
                    .then
                    .as_ref()
                    .ok_or_else(|| ConvertError::MissingField("then result".to_string()))?;
                Ok(WhenClause::new(
                    self.from_substrait(cond)?,
                    self.from_substrait(result)?,
                ))
            })
            .collect();

        let else_expr = if let Some(e) = &if_then.r#else {
            Some(self.from_substrait(e)?)
        } else {
            None
        };

        Ok(Expr::case(when_then?, else_expr))
    }

    fn from_cast(
        &self,
        cast: &expression::Cast,
    ) -> Result<Expr, ConvertError> {
        let input = cast
            .input
            .as_ref()
            .ok_or_else(|| ConvertError::MissingField("cast input".to_string()))?;
        let inner = self.from_substrait(input)?;

        let data_type = cast
            .r#type
            .as_ref()
            .map(super::substrait_to_datatype)
            .unwrap_or(DataType::String);

        Ok(Expr::cast(inner, data_type))
    }

    fn from_singular_or_list(
        &self,
        sol: &expression::SingularOrList,
    ) -> Result<Expr, ConvertError> {
        let value = sol
            .value
            .as_ref()
            .ok_or_else(|| ConvertError::MissingField("SingularOrList value".to_string()))?;
        let expr = self.from_substrait(value)?;
        let list: Result<Vec<_>, _> = sol.options.iter().map(|e| self.from_substrait(e)).collect();
        Ok(Expr::in_list(expr, list?))
    }
}


/// Convert a Substrait literal to Expr (no schema context needed).
fn from_literal(lit: &proto::expression::Literal) -> Result<Expr, ConvertError> {
    use expression::literal::LiteralType;
    match &lit.literal_type {
        Some(LiteralType::Boolean(b)) => Ok(Expr::boolean(*b)),
        Some(LiteralType::I32(i)) => Ok(Expr::int(*i as i64)),
        Some(LiteralType::I64(i)) => Ok(Expr::int(*i)),
        Some(LiteralType::Fp32(f)) => Ok(Expr::float(*f as f64)),
        Some(LiteralType::Fp64(f)) => Ok(Expr::float(*f)),
        Some(LiteralType::String(s)) => Ok(Expr::string(s.clone())),
        Some(LiteralType::Null(_)) => Ok(Expr::null()),
        _ => Err(ConvertError::UnsupportedExpression(
            "Unsupported literal type".to_string(),
        )),
    }
}

/// Build a Substrait ScalarFunction expression from anchor + args.
fn scalar_function(function_ref: u32, args: Vec<proto::Expression>) -> proto::Expression {
    let arguments: Vec<_> = args
        .into_iter()
        .map(|expr| proto::FunctionArgument {
            arg_type: Some(ArgType::Value(expr)),
        })
        .collect();

    proto::Expression {
        rex_type: Some(expression::RexType::ScalarFunction(
            proto::expression::ScalarFunction {
                function_reference: function_ref,
                arguments,
                output_type: None,
                options: vec![],
                #[allow(deprecated)]
                args: vec![],
            },
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Field;
    use semstrait_core::DataType;

    #[test]
    fn test_column_reference() {
        let schema = Schema::new(vec![
            Field::new("id", DataType::Integer),
            Field::new("amount", DataType::Number),
        ]);

        let converter = ExprConverter::new(&schema);
        let expr = Expr::column("amount");

        let substrait = converter.to_substrait(&expr).unwrap();
        let back = converter.from_substrait(&substrait).unwrap();

        assert_eq!(expr, back);
    }

    #[test]
    fn test_integer_literal_roundtrip() {
        let schema = Schema::empty();
        let converter = ExprConverter::new(&schema);

        let expr = Expr::int(42);
        let substrait = converter.to_substrait(&expr).unwrap();
        let back = converter.from_substrait(&substrait).unwrap();
        assert_eq!(back, Expr::int(42));
    }

    #[test]
    fn test_float_literal_roundtrip() {
        let schema = Schema::empty();
        let converter = ExprConverter::new(&schema);

        let expr = Expr::float(2.72);
        let substrait = converter.to_substrait(&expr).unwrap();
        let back = converter.from_substrait(&substrait).unwrap();
        assert_eq!(back, Expr::float(2.72));
    }

    #[test]
    fn test_literals() {
        let schema = Schema::empty();
        let converter = ExprConverter::new(&schema);

        let tests = vec![
            Expr::string("hello"),
            Expr::boolean(true),
            Expr::null(),
        ];

        for expr in tests {
            let substrait = converter.to_substrait(&expr).unwrap();
            let back = converter.from_substrait(&substrait).unwrap();
            assert_eq!(expr, back);
        }
    }

    #[test]
    fn test_binary_op() {
        let schema = Schema::new(vec![
            Field::new("a", DataType::Integer),
            Field::new("b", DataType::Integer),
        ]);

        let converter = ExprConverter::new(&schema);
        let expr = Expr::eq(Expr::column("a"), Expr::int(10));

        let substrait = converter.to_substrait(&expr).unwrap();
        let back = converter.from_substrait(&substrait).unwrap();

        assert_eq!(expr, back);
    }

    #[test]
    fn test_not_roundtrip() {
        let schema = Schema::new(vec![Field::new("active", DataType::Boolean)]);
        let converter = ExprConverter::new(&schema);

        let expr = Expr::not(Expr::column("active"));

        let substrait = converter.to_substrait(&expr).unwrap();
        let back = converter.from_substrait(&substrait).unwrap();

        assert_eq!(expr, back);
    }

    #[test]
    fn test_is_null_roundtrip() {
        let schema = Schema::new(vec![Field::new("value", DataType::Number)]);
        let converter = ExprConverter::new(&schema);

        let expr = Expr::is_null(Expr::column("value"));

        let substrait = converter.to_substrait(&expr).unwrap();
        let back = converter.from_substrait(&substrait).unwrap();

        assert_eq!(expr, back);
    }

    #[test]
    fn test_is_not_null_roundtrip() {
        let schema = Schema::new(vec![Field::new("value", DataType::Number)]);
        let converter = ExprConverter::new(&schema);

        let expr = Expr::is_not_null(Expr::column("value"));

        let substrait = converter.to_substrait(&expr).unwrap();
        let back = converter.from_substrait(&substrait).unwrap();

        assert_eq!(expr, back);
    }

    #[test]
    fn test_in_list_roundtrip() {
        let schema = Schema::new(vec![Field::new("status", DataType::String)]);
        let converter = ExprConverter::new(&schema);

        let expr = Expr::in_list(
            Expr::column("status"),
            vec![Expr::string("active"), Expr::string("pending")],
        );

        let substrait = converter.to_substrait(&expr).unwrap();
        let back = converter.from_substrait(&substrait).unwrap();

        assert_eq!(expr, back);
    }

    #[test]
    fn test_between_roundtrip() {
        let schema = Schema::new(vec![Field::new("age", DataType::Integer)]);
        let converter = ExprConverter::new(&schema);

        let expr = Expr::between(Expr::column("age"), Expr::int(18), Expr::int(65));

        let substrait = converter.to_substrait(&expr).unwrap();
        let back = converter.from_substrait(&substrait).unwrap();

        assert_eq!(expr, back);
    }

    #[test]
    fn test_like_roundtrip() {
        let schema = Schema::new(vec![Field::new("name", DataType::String)]);
        let converter = ExprConverter::new(&schema);

        let expr = Expr::like(Expr::column("name"), Expr::string("%smith%"));

        let substrait = converter.to_substrait(&expr).unwrap();
        let back = converter.from_substrait(&substrait).unwrap();

        assert_eq!(expr, back);
    }

    #[test]
    fn test_coalesce_roundtrip() {
        let schema = Schema::new(vec![
            Field::new("primary", DataType::Number),
            Field::new("fallback", DataType::Number),
        ]);
        let converter = ExprConverter::new(&schema);

        let expr = Expr::coalesce(vec![
            Expr::column("primary"),
            Expr::column("fallback"),
            Expr::float(0.0),
        ]);

        let substrait = converter.to_substrait(&expr).unwrap();
        let back = converter.from_substrait(&substrait).unwrap();

        assert_eq!(expr, back);
    }

    #[test]
    fn test_nullif_roundtrip() {
        let schema = Schema::new(vec![Field::new("value", DataType::Number)]);
        let converter = ExprConverter::new(&schema);

        let expr = Expr::null_if(Expr::column("value"), Expr::float(0.0));

        let substrait = converter.to_substrait(&expr).unwrap();
        let back = converter.from_substrait(&substrait).unwrap();

        assert_eq!(expr, back);
    }

    #[test]
    fn test_case_roundtrip() {
        let schema = Schema::new(vec![
            Field::new("age", DataType::Integer),
            Field::new("status", DataType::String),
        ]);
        let converter = ExprConverter::new(&schema);

        let expr = Expr::case(
            vec![
                WhenClause::new(
                    Expr::lt(Expr::column("age"), Expr::int(18)),
                    Expr::string("minor"),
                ),
                WhenClause::new(
                    Expr::gt(Expr::column("age"), Expr::int(65)),
                    Expr::string("senior"),
                ),
            ],
            Some(Expr::string("adult")),
        );

        let substrait = converter.to_substrait(&expr).unwrap();
        let back = converter.from_substrait(&substrait).unwrap();

        assert_eq!(expr, back);
    }

    #[test]
    fn test_ilike_roundtrip() {
        let schema = Schema::new(vec![Field::new("name", DataType::String)]);
        let converter = ExprConverter::new(&schema);

        let expr = Expr::ilike(Expr::column("name"), Expr::string("%smith%"));

        let substrait = converter.to_substrait(&expr).unwrap();
        let back = converter.from_substrait(&substrait).unwrap();

        assert_eq!(expr, back);
    }

    #[test]
    fn test_regexp_match_roundtrip() {
        let schema = Schema::new(vec![Field::new("email", DataType::String)]);
        let converter = ExprConverter::new(&schema);

        let expr = Expr::regexp_match(
            Expr::column("email"),
            Expr::string("@example\\.com"),
            false,
        );

        let substrait = converter.to_substrait(&expr).unwrap();
        let back = converter.from_substrait(&substrait).unwrap();

        assert_eq!(expr, back);
    }

    #[test]
    fn test_cast_roundtrip() {
        let schema = Schema::new(vec![Field::new("amount", DataType::Number)]);
        let converter = ExprConverter::new(&schema);

        let expr = Expr::cast(Expr::column("amount"), DataType::String);

        let substrait = converter.to_substrait(&expr).unwrap();
        let back = converter.from_substrait(&substrait).unwrap();

        assert_eq!(expr, back);
    }

    #[test]
    fn test_regexp_extract_roundtrip() {
        let schema = Schema::new(vec![Field::new("campaign", DataType::String)]);
        let converter = ExprConverter::new(&schema);

        let expr = Expr::regexp_extract(
            Expr::column("campaign"),
            Expr::string("^([A-Z]{2})_"),
            1,
        );

        let substrait = converter.to_substrait(&expr).unwrap();
        let back = converter.from_substrait(&substrait).unwrap();

        assert_eq!(expr, back);
    }
}
