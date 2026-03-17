//! DslExpr <-> Substrait Expression converter

use crate::error::ConvertError;
use crate::plan_node::{BinaryOp, DslExpr};
use crate::schema::Schema;
use substrait::proto::{
    self,
    expression::{
        self, literal::LiteralType, reference_segment::ReferenceType, ReferenceSegment,
    },
    function_argument::ArgType,
    r#type::{Kind, Nullability},
};

// Function anchors for scalar functions
const FUNC_EQUAL: u32 = 100;
const FUNC_NOT_EQUAL: u32 = 101;
const FUNC_LT: u32 = 102;
const FUNC_LTE: u32 = 103;
const FUNC_GT: u32 = 104;
const FUNC_GTE: u32 = 105;
const FUNC_AND: u32 = 200;
const FUNC_OR: u32 = 201;
const FUNC_ADD: u32 = 300;
const FUNC_SUBTRACT: u32 = 301;
const FUNC_MULTIPLY: u32 = 302;
const FUNC_DIVIDE: u32 = 303;

/// Converts DslExpr to/from Substrait Expression
pub struct ExprConverter<'s> {
    schema: &'s Schema,
}

impl<'s> ExprConverter<'s> {
    pub fn new(schema: &'s Schema) -> Self {
        Self { schema }
    }

    /// Convert DslExpr to Substrait Expression
    pub fn to_substrait(&self, expr: &DslExpr) -> Result<proto::Expression, ConvertError> {
        match expr {
            DslExpr::Column { name, qualifier } => {
                let field_name = if let Some(q) = qualifier {
                    format!("{}.{}", q, name)
                } else {
                    name.clone()
                };

                let ordinal = self
                    .schema
                    .ordinal(&field_name)
                    .or_else(|| self.schema.ordinal(name))
                    .ok_or_else(|| ConvertError::ColumnNotFound(field_name.clone()))?;

                self.field_reference(ordinal as u32)
            }

            DslExpr::Number(n) => Ok(self.literal_f64(*n)),

            DslExpr::StringLit(s) => Ok(self.literal_string(s)),

            DslExpr::Bool(b) => Ok(self.literal_bool(*b)),

            DslExpr::Null => Ok(self.literal_null()),

            DslExpr::BinaryOp { left, op, right } => {
                let left_expr = self.to_substrait(left)?;
                let right_expr = self.to_substrait(right)?;
                self.binary_op(left_expr, *op, right_expr)
            }

            DslExpr::FunctionCall {
                name,
                args,
                distinct: _,
            } => {
                let arg_exprs: Result<Vec<_>, _> =
                    args.iter().map(|a| self.to_substrait(a)).collect();
                let arg_exprs = arg_exprs?;
                self.function_call(name, arg_exprs)
            }

            DslExpr::Negate(inner) => {
                let inner_expr = self.to_substrait(inner)?;
                // Negate as: 0 - expr
                let zero = self.literal_f64(0.0);
                self.binary_op(zero, BinaryOp::Subtract, inner_expr)
            }

            DslExpr::Case {
                when_then,
                else_expr,
            } => {
                let ifs: Result<Vec<_>, _> = when_then
                    .iter()
                    .map(|(cond, result)| {
                        let if_expr = self.to_substrait(cond)?;
                        let then_expr = self.to_substrait(result)?;
                        Ok(expression::if_then::IfClause {
                            r#if: Some(if_expr),
                            then: Some(then_expr),
                        })
                    })
                    .collect();
                let ifs = ifs?;

                let else_result = if let Some(e) = else_expr {
                    Some(Box::new(self.to_substrait(e)?))
                } else {
                    None
                };

                Ok(proto::Expression {
                    rex_type: Some(expression::RexType::IfThen(Box::new(
                        expression::IfThen {
                            ifs,
                            r#else: else_result,
                        },
                    ))),
                })
            }

            // New variants — mapped to Substrait scalar functions for now.
            DslExpr::Not(inner) => {
                let inner_expr = self.to_substrait(inner)?;
                self.function_call("not", vec![inner_expr])
            }

            DslExpr::IsNull(inner) => {
                let inner_expr = self.to_substrait(inner)?;
                self.function_call("is_null", vec![inner_expr])
            }

            DslExpr::IsNotNull(inner) => {
                let inner_expr = self.to_substrait(inner)?;
                self.function_call("is_not_null", vec![inner_expr])
            }

            DslExpr::InList { expr, list, .. } => {
                let mut args = vec![self.to_substrait(expr)?];
                for item in list {
                    args.push(self.to_substrait(item)?);
                }
                self.function_call("in", args)
            }

            DslExpr::Between { expr, low, high, .. } => {
                let args = vec![
                    self.to_substrait(expr)?,
                    self.to_substrait(low)?,
                    self.to_substrait(high)?,
                ];
                self.function_call("between", args)
            }

            DslExpr::Like { expr, pattern } => {
                let args = vec![
                    self.to_substrait(expr)?,
                    self.to_substrait(pattern)?,
                ];
                self.function_call("like", args)
            }

            DslExpr::Coalesce(exprs) => {
                let args: Result<Vec<_>, _> =
                    exprs.iter().map(|e| self.to_substrait(e)).collect();
                self.function_call("coalesce", args?)
            }

            DslExpr::NullIf { expr, null_expr } => {
                let args = vec![
                    self.to_substrait(expr)?,
                    self.to_substrait(null_expr)?,
                ];
                self.function_call("nullif", args)
            }

            DslExpr::DateTrunc { grain, expr } => {
                let grain_expr = self.literal_string(grain);
                let inner_expr = self.to_substrait(expr)?;
                self.function_call("date_trunc", vec![grain_expr, inner_expr])
            }
        }
    }

    /// Convert Substrait Expression to DslExpr (basic support)
    pub fn from_substrait(
        &self,
        expr: &proto::Expression,
    ) -> Result<DslExpr, ConvertError> {
        match &expr.rex_type {
            Some(expression::RexType::Literal(lit)) => self.from_literal(lit),

            Some(expression::RexType::Selection(field_ref)) => {
                self.from_field_reference(field_ref)
            }

            Some(expression::RexType::ScalarFunction(func)) => {
                self.from_scalar_function(func)
            }

            Some(expression::RexType::IfThen(if_then)) => self.from_if_then(if_then),

            _ => Err(ConvertError::UnsupportedExpression(format!(
                "Unsupported expression type: {:?}",
                expr.rex_type
            ))),
        }
    }

    // Helper: create field reference
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

    // Helper: create literal expressions
    fn literal_f64(&self, value: f64) -> proto::Expression {
        proto::Expression {
            rex_type: Some(expression::RexType::Literal(proto::expression::Literal {
                nullable: true,
                type_variation_reference: 0,
                literal_type: Some(LiteralType::Fp64(value)),
            })),
        }
    }

    fn literal_string(&self, value: &str) -> proto::Expression {
        proto::Expression {
            rex_type: Some(expression::RexType::Literal(proto::expression::Literal {
                nullable: true,
                type_variation_reference: 0,
                literal_type: Some(LiteralType::String(value.to_string())),
            })),
        }
    }

    fn literal_bool(&self, value: bool) -> proto::Expression {
        proto::Expression {
            rex_type: Some(expression::RexType::Literal(proto::expression::Literal {
                nullable: true,
                type_variation_reference: 0,
                literal_type: Some(LiteralType::Boolean(value)),
            })),
        }
    }

    fn literal_null(&self) -> proto::Expression {
        proto::Expression {
            rex_type: Some(expression::RexType::Literal(proto::expression::Literal {
                nullable: true,
                type_variation_reference: 0,
                literal_type: Some(LiteralType::Null(proto::Type {
                    kind: Some(Kind::Bool(proto::r#type::Boolean {
                        type_variation_reference: 0,
                        nullability: Nullability::Nullable as i32,
                    })),
                })),
            })),
        }
    }

    // Helper: binary operation
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
            BinaryOp::SafeDivide => FUNC_DIVIDE, // Same substrait func; null guard at SQL level
        };

        Ok(proto::Expression {
            rex_type: Some(expression::RexType::ScalarFunction(
                proto::expression::ScalarFunction {
                    function_reference: function_ref,
                    arguments: vec![
                        proto::FunctionArgument {
                            arg_type: Some(ArgType::Value(left)),
                        },
                        proto::FunctionArgument {
                            arg_type: Some(ArgType::Value(right)),
                        },
                    ],
                    output_type: None,
                    options: vec![],
                    args: vec![],
                },
            )),
        })
    }

    // Helper: function call
    fn function_call(
        &self,
        name: &str,
        args: Vec<proto::Expression>,
    ) -> Result<proto::Expression, ConvertError> {
        // Map common function names to anchors
        let function_ref = match name.to_lowercase().as_str() {
            "coalesce" => 204,
            "is_null" => 202,
            "is_not_null" => 203,
            _ => {
                return Err(ConvertError::FunctionNotFound(format!(
                    "Function not mapped: {}",
                    name
                )))
            }
        };

        let arguments: Vec<_> = args
            .into_iter()
            .map(|expr| proto::FunctionArgument {
                arg_type: Some(ArgType::Value(expr)),
            })
            .collect();

        Ok(proto::Expression {
            rex_type: Some(expression::RexType::ScalarFunction(
                proto::expression::ScalarFunction {
                    function_reference: function_ref,
                    arguments,
                    output_type: None,
                    options: vec![],
                    args: vec![],
                },
            )),
        })
    }

    // Deserialize helpers
    fn from_literal(
        &self,
        lit: &proto::expression::Literal,
    ) -> Result<DslExpr, ConvertError> {
        match &lit.literal_type {
            Some(LiteralType::Boolean(b)) => Ok(DslExpr::Bool(*b)),
            Some(LiteralType::I32(i)) => Ok(DslExpr::Number(*i as f64)),
            Some(LiteralType::I64(i)) => Ok(DslExpr::Number(*i as f64)),
            Some(LiteralType::Fp32(f)) => Ok(DslExpr::Number(*f as f64)),
            Some(LiteralType::Fp64(f)) => Ok(DslExpr::Number(*f)),
            Some(LiteralType::String(s)) => Ok(DslExpr::StringLit(s.clone())),
            Some(LiteralType::Null(_)) => Ok(DslExpr::Null),
            _ => Err(ConvertError::UnsupportedExpression(
                "Unsupported literal type".to_string(),
            )),
        }
    }

    fn from_field_reference(
        &self,
        field_ref: &proto::expression::FieldReference,
    ) -> Result<DslExpr, ConvertError> {
        // Extract field index from StructField
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

        Ok(DslExpr::Column {
            name: field.name.clone(),
            qualifier: None,
        })
    }

    fn from_scalar_function(
        &self,
        func: &proto::expression::ScalarFunction,
    ) -> Result<DslExpr, ConvertError> {
        // Extract arguments
        let args: Result<Vec<_>, _> = func
            .arguments
            .iter()
            .filter_map(|arg| match &arg.arg_type {
                Some(ArgType::Value(expr)) => Some(self.from_substrait(expr)),
                _ => None,
            })
            .collect();
        let args = args?;

        // Map function reference back to operator or function name
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
            202 => Ok(DslExpr::FunctionCall {
                name: "is_null".to_string(),
                args,
                distinct: false,
            }),
            203 => Ok(DslExpr::FunctionCall {
                name: "is_not_null".to_string(),
                args,
                distinct: false,
            }),
            204 => Ok(DslExpr::FunctionCall {
                name: "coalesce".to_string(),
                args,
                distinct: false,
            }),
            _ => Err(ConvertError::FunctionNotFound(format!(
                "Unknown function reference: {}",
                func.function_reference
            ))),
        }
    }

    fn binary_from_args(
        &self,
        mut args: Vec<DslExpr>,
        op: BinaryOp,
    ) -> Result<DslExpr, ConvertError> {
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
        Ok(DslExpr::BinaryOp {
            left: Box::new(left),
            op,
            right: Box::new(right),
        })
    }

    fn from_if_then(
        &self,
        if_then: &expression::IfThen,
    ) -> Result<DslExpr, ConvertError> {
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
                Ok((self.from_substrait(cond)?, self.from_substrait(result)?))
            })
            .collect();
        let when_then = when_then?;

        let else_expr = if let Some(e) = &if_then.r#else {
            Some(Box::new(self.from_substrait(e)?))
        } else {
            None
        };

        Ok(DslExpr::Case {
            when_then,
            else_expr,
        })
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
            Field::new("id", DataType::Int64),
            Field::new("amount", DataType::Float64),
        ]);

        let converter = ExprConverter::new(&schema);
        let expr = DslExpr::Column {
            name: "amount".to_string(),
            qualifier: None,
        };

        let substrait = converter.to_substrait(&expr).unwrap();
        let back = converter.from_substrait(&substrait).unwrap();

        assert_eq!(expr, back);
    }

    #[test]
    fn test_literals() {
        let schema = Schema::empty();
        let converter = ExprConverter::new(&schema);

        let tests = vec![
            DslExpr::Number(42.0),
            DslExpr::StringLit("hello".to_string()),
            DslExpr::Bool(true),
            DslExpr::Null,
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
            Field::new("a", DataType::Int64),
            Field::new("b", DataType::Int64),
        ]);

        let converter = ExprConverter::new(&schema);
        let expr = DslExpr::BinaryOp {
            left: Box::new(DslExpr::Column {
                name: "a".to_string(),
                qualifier: None,
            }),
            op: BinaryOp::Eq,
            right: Box::new(DslExpr::Number(10.0)),
        };

        let substrait = converter.to_substrait(&expr).unwrap();
        let back = converter.from_substrait(&substrait).unwrap();

        assert_eq!(expr, back);
    }
}
