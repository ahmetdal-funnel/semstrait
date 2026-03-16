//! Substrait plan emission

use substrait::proto::{
    self,
    plan_rel::RelType as PlanRelType,
    rel::RelType,
    read_rel::{NamedTable, ReadType},
    expression::{
        self,
        literal::LiteralType,
        reference_segment::{ReferenceType, StructField},
        ReferenceSegment,
    },
    aggregate_rel::{Grouping, Measure},
    aggregate_function::AggregationInvocation,
    function_argument::ArgType,
    extensions::{
        SimpleExtensionUri,
        SimpleExtensionDeclaration,
        simple_extension_declaration::MappingType,
    },
    rel_common::EmitKind,
    AggregationPhase,
    r#type::{Kind, Nullability},
};

use crate::planner::ir::plan_node::{
    PlanNode, Scan, Join, JoinType, CrossJoin, Filter, Aggregate, Project, Sort, SortDirection, Union,
    VirtualTable, LiteralValue,
};
use crate::planner::ir::expr::{
    Expr, Column, Literal, BinaryOperator, AggregateExpr,
};
use crate::schema::Aggregation;
use crate::planner::ir::error::EmitError;

// Extension URI anchors
const URI_AGGREGATE: u32 = 1;
const URI_COMPARISON: u32 = 2;
const URI_BOOLEAN: u32 = 3;
const URI_ARITHMETIC: u32 = 4;

// Standard Substrait extension URIs
const URI_AGGREGATE_GENERIC: &str = "/functions_aggregate_generic.yaml";
const URI_COMPARISON_FUNCTIONS: &str = "/functions_comparison.yaml";
const URI_BOOLEAN_FUNCTIONS: &str = "/functions_boolean.yaml";
const URI_ARITHMETIC_FUNCTIONS: &str = "/functions_arithmetic.yaml";

// Aggregate function anchors (must match what we use in emit_measure)
const FUNC_SUM: u32 = 1;
const FUNC_AVG: u32 = 2;
const FUNC_COUNT: u32 = 3;
const FUNC_COUNT_DISTINCT: u32 = 4;
const FUNC_MIN: u32 = 5;
const FUNC_MAX: u32 = 6;

// Comparison function anchors (must match what we use in emit_binary_expr)
const FUNC_EQUAL: u32 = 100;
const FUNC_NOT_EQUAL: u32 = 101;
const FUNC_LT: u32 = 102;
const FUNC_LTE: u32 = 103;
const FUNC_GT: u32 = 104;
const FUNC_GTE: u32 = 105;

// Boolean function anchors
const FUNC_AND: u32 = 200;
const FUNC_OR: u32 = 201;
const FUNC_IS_NULL: u32 = 202;
const FUNC_IS_NOT_NULL: u32 = 203;
const FUNC_COALESCE: u32 = 204;

// Arithmetic function anchors
const FUNC_ADD: u32 = 300;
const FUNC_SUBTRACT: u32 = 301;
const FUNC_MULTIPLY: u32 = 302;
const FUNC_DIVIDE: u32 = 303;

/// Schema context for tracking column positions across the plan
#[derive(Debug, Clone)]
struct SchemaContext {
    /// List of (table_alias, column_name) pairs in order
    columns: Vec<(String, String)>,
}

impl SchemaContext {
    fn new() -> Self {
        Self { columns: Vec::new() }
    }

    /// Add columns from a scan
    fn add_scan(&mut self, alias: &str, columns: &[String]) {
        for col in columns {
            self.columns.push((alias.to_string(), col.clone()));
        }
    }

    /// Merge another context (for joins)
    fn merge(&mut self, other: SchemaContext) {
        self.columns.extend(other.columns);
    }

    /// Find the index of a column by table alias and column name
    fn find_column(&self, table: &str, name: &str) -> Option<usize> {
        self.columns.iter().position(|(t, c)| t == table && c == name)
    }

    /// Get total column count
    fn len(&self) -> usize {
        self.columns.len()
    }
}

/// Build extension URI declarations
fn build_extension_uris() -> Vec<SimpleExtensionUri> {
    vec![
        SimpleExtensionUri {
            extension_uri_anchor: URI_AGGREGATE,
            uri: URI_AGGREGATE_GENERIC.to_string(),
        },
        SimpleExtensionUri {
            extension_uri_anchor: URI_COMPARISON,
            uri: URI_COMPARISON_FUNCTIONS.to_string(),
        },
        SimpleExtensionUri {
            extension_uri_anchor: URI_BOOLEAN,
            uri: URI_BOOLEAN_FUNCTIONS.to_string(),
        },
        SimpleExtensionUri {
            extension_uri_anchor: URI_ARITHMETIC,
            uri: URI_ARITHMETIC_FUNCTIONS.to_string(),
        },
    ]
}

/// Build function extension declarations
fn build_extensions() -> Vec<SimpleExtensionDeclaration> {
    vec![
        // Aggregate functions
        make_function_extension(URI_AGGREGATE, FUNC_SUM, "sum"),
        make_function_extension(URI_AGGREGATE, FUNC_AVG, "avg"),
        make_function_extension(URI_AGGREGATE, FUNC_COUNT, "count"),
        make_function_extension(URI_AGGREGATE, FUNC_COUNT_DISTINCT, "count"),  // count with distinct flag
        make_function_extension(URI_AGGREGATE, FUNC_MIN, "min"),
        make_function_extension(URI_AGGREGATE, FUNC_MAX, "max"),
        // Comparison functions
        make_function_extension(URI_COMPARISON, FUNC_EQUAL, "equal"),
        make_function_extension(URI_COMPARISON, FUNC_NOT_EQUAL, "not_equal"),
        make_function_extension(URI_COMPARISON, FUNC_LT, "lt"),
        make_function_extension(URI_COMPARISON, FUNC_LTE, "lte"),
        make_function_extension(URI_COMPARISON, FUNC_GT, "gt"),
        make_function_extension(URI_COMPARISON, FUNC_GTE, "gte"),
        // Boolean functions
        make_function_extension(URI_BOOLEAN, FUNC_AND, "and"),
        make_function_extension(URI_BOOLEAN, FUNC_COALESCE, "coalesce"),
        // Arithmetic functions
        make_function_extension(URI_ARITHMETIC, FUNC_ADD, "add"),
        make_function_extension(URI_ARITHMETIC, FUNC_SUBTRACT, "subtract"),
        make_function_extension(URI_ARITHMETIC, FUNC_MULTIPLY, "multiply"),
        make_function_extension(URI_ARITHMETIC, FUNC_DIVIDE, "divide"),
    ]
}

/// Helper to create a function extension declaration
#[allow(deprecated)]
fn make_function_extension(uri_ref: u32, anchor: u32, name: &str) -> SimpleExtensionDeclaration {
    SimpleExtensionDeclaration {
        mapping_type: Some(MappingType::ExtensionFunction(
            proto::extensions::simple_extension_declaration::ExtensionFunction {
                extension_uri_reference: uri_ref,
                extension_urn_reference: uri_ref,  // URN replaces URI in newer versions
                function_anchor: anchor,
                name: name.to_string(),
            }
        )),
    }
}

/// Emit a Substrait Plan from a PlanNode
///
/// If `output_names` is provided, those semantic names will be used for the output columns.
/// Otherwise, physical column names (table.column) are used.
#[allow(deprecated)]
pub fn emit_plan(node: &PlanNode, output_names: Option<Vec<String>>) -> Result<proto::Plan, EmitError> {
    let mut ctx = SchemaContext::new();
    let rel = emit_rel(node, &mut ctx)?;

    // Use provided semantic names or fall back to physical names
    let names = output_names.unwrap_or_else(|| {
        ctx.columns.iter()
            .map(|(t, c)| format!("{}.{}", t, c))
            .collect()
    });

    Ok(proto::Plan {
        version: Some(proto::Version {
            major_number: 0,
            minor_number: 62,
            patch_number: 0,
            ..Default::default()
        }),
        // Use both deprecated and new fields for compatibility
        extension_uris: build_extension_uris(),
        extensions: build_extensions(),
        relations: vec![proto::PlanRel {
            rel_type: Some(PlanRelType::Root(proto::RelRoot {
                input: Some(rel),
                names,
            })),
        }],
        ..Default::default()
    })
}

/// Emit a Rel from a PlanNode, updating the schema context
fn emit_rel(node: &PlanNode, ctx: &mut SchemaContext) -> Result<proto::Rel, EmitError> {
    match node {
        PlanNode::Scan(scan) => emit_scan(scan, ctx),
        PlanNode::Join(join) => emit_join(join, ctx),
        PlanNode::CrossJoin(cross) => emit_cross_join(cross, ctx),
        PlanNode::Filter(filter) => emit_filter(filter, ctx),
        PlanNode::Aggregate(agg) => emit_aggregate(agg, ctx),
        PlanNode::Project(proj) => emit_project(proj, ctx),
        PlanNode::Sort(sort) => emit_sort(sort, ctx),
        PlanNode::Union(union) => emit_union(union, ctx),
        PlanNode::VirtualTable(vt) => emit_virtual_table(vt, ctx),
    }
}

/// Convert a type name string to Substrait Type
fn type_to_substrait(type_name: &str) -> proto::Type {
    let lower = type_name.to_lowercase();

    // Check for decimal type with precision and scale: decimal(p, s)
    if let Some(kind) = parse_decimal_type(&lower) {
        return proto::Type { kind: Some(kind) };
    }

    let kind = match lower.as_str() {
        "i8" => Kind::I8(proto::r#type::I8 {
            type_variation_reference: 0,
            nullability: Nullability::Nullable as i32,
        }),
        "i16" => Kind::I16(proto::r#type::I16 {
            type_variation_reference: 0,
            nullability: Nullability::Nullable as i32,
        }),
        "i32" | "int" | "integer" => Kind::I32(proto::r#type::I32 {
            type_variation_reference: 0,
            nullability: Nullability::Nullable as i32,
        }),
        "i64" | "long" | "bigint" => Kind::I64(proto::r#type::I64 {
            type_variation_reference: 0,
            nullability: Nullability::Nullable as i32,
        }),
        "f32" | "float" => Kind::Fp32(proto::r#type::Fp32 {
            type_variation_reference: 0,
            nullability: Nullability::Nullable as i32,
        }),
        "f64" | "double" => Kind::Fp64(proto::r#type::Fp64 {
            type_variation_reference: 0,
            nullability: Nullability::Nullable as i32,
        }),
        "bool" | "boolean" => Kind::Bool(proto::r#type::Boolean {
            type_variation_reference: 0,
            nullability: Nullability::Nullable as i32,
        }),
        "date" => Kind::Date(proto::r#type::Date {
            type_variation_reference: 0,
            nullability: Nullability::Nullable as i32,
        }),
        "timestamp" => Kind::PrecisionTimestamp(proto::r#type::PrecisionTimestamp {
            precision: 6,
            type_variation_reference: 0,
            nullability: Nullability::Nullable as i32,
        }),
        _ => Kind::String(proto::r#type::String {
            type_variation_reference: 0,
            nullability: Nullability::Nullable as i32,
        }),
    };
    proto::Type { kind: Some(kind) }
}

/// Parse decimal type string like "decimal(31, 7)" and return Substrait Decimal Kind
fn parse_decimal_type(type_str: &str) -> Option<Kind> {
    // Match "decimal(precision, scale)" or "decimal(precision,scale)"
    if !type_str.starts_with("decimal(") || !type_str.ends_with(")") {
        return None;
    }

    // Extract the part between parentheses
    let inner = &type_str[8..type_str.len() - 1];
    let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();

    if parts.len() != 2 {
        return None;
    }

    let precision: i32 = parts[0].parse().ok()?;
    let scale: i32 = parts[1].parse().ok()?;

    Some(Kind::Decimal(proto::r#type::Decimal {
        precision,
        scale,
        type_variation_reference: 0,
        nullability: Nullability::Nullable as i32,
    }))
}

/// Emit a ReadRel (table scan) with base_schema
fn emit_scan(scan: &Scan, ctx: &mut SchemaContext) -> Result<proto::Rel, EmitError> {
    let alias = scan.reference_name();

    // Build the base schema with column names and types
    let types: Vec<proto::Type> = scan.columns.iter().enumerate().map(|(i, _)| {
        let type_name = scan.column_type(i);
        type_to_substrait(type_name)
    }).collect();

    let base_schema = proto::NamedStruct {
        names: scan.columns.clone(),
        r#struct: Some(proto::r#type::Struct {
            types,
            type_variation_reference: 0,
            nullability: Nullability::Nullable as i32,
        }),
    };

    // Add columns to context
    ctx.add_scan(alias, &scan.columns);

    // Split table name into parts for Substrait (e.g., "schema.table" -> ["schema", "table"])
    let table_names: Vec<String> = scan.table.split('.').map(String::from).collect();

    Ok(proto::Rel {
        rel_type: Some(RelType::Read(Box::new(proto::ReadRel {
            read_type: Some(ReadType::NamedTable(NamedTable {
                names: table_names,
                advanced_extension: None,
            })),
            base_schema: Some(base_schema),
            ..Default::default()
        }))),
    })
}

/// Emit a JoinRel
fn emit_join(join: &Join, ctx: &mut SchemaContext) -> Result<proto::Rel, EmitError> {
    // Emit left side first
    let mut left_ctx = SchemaContext::new();
    let left = emit_rel(&join.left, &mut left_ctx)?;

    // Emit right side
    let mut right_ctx = SchemaContext::new();
    let right = emit_rel(&join.right, &mut right_ctx)?;

    // Merge contexts - left columns come first, then right
    ctx.merge(left_ctx.clone());
    let right_offset = ctx.len();
    ctx.merge(right_ctx.clone());

    let join_type = match join.join_type {
        JoinType::Inner => proto::join_rel::JoinType::Inner,
        JoinType::Left => proto::join_rel::JoinType::Left,
        JoinType::Right => proto::join_rel::JoinType::Right,
        JoinType::Full => proto::join_rel::JoinType::Outer,
    };

    // Build join condition from all key pairs (AND-ed together)
    let mut conditions: Vec<proto::Expression> = Vec::new();
    for (lk, rk) in join.left_keys.iter().zip(join.right_keys.iter()) {
        let left_idx = left_ctx.find_column(&lk.table, &lk.name)
            .ok_or_else(|| EmitError::ColumnNotFound(lk.qualified_name()))?;
        let right_idx = right_ctx.find_column(&rk.table, &rk.name)
            .ok_or_else(|| EmitError::ColumnNotFound(rk.qualified_name()))?;
        conditions.push(emit_binary_expr_with_indices(
            left_idx as u32,
            BinaryOperator::Eq,
            (right_offset + right_idx) as u32,
        ));
    }
    let condition = if conditions.len() == 1 {
        conditions.into_iter().next().unwrap()
    } else {
        // AND all conditions together
        conditions.into_iter().reduce(|acc, expr| {
            emit_and_expr(acc, expr)
        }).unwrap()
    };

    Ok(proto::Rel {
        rel_type: Some(RelType::Join(Box::new(proto::JoinRel {
            left: Some(Box::new(left)),
            right: Some(Box::new(right)),
            r#type: join_type as i32,
            expression: Some(Box::new(condition)),
            ..Default::default()
        }))),
    })
}

/// Emit a CrossRel (cartesian product, no join condition)
fn emit_cross_join(cross: &CrossJoin, ctx: &mut SchemaContext) -> Result<proto::Rel, EmitError> {
    let mut left_ctx = SchemaContext::new();
    let left = emit_rel(&cross.left, &mut left_ctx)?;

    let mut right_ctx = SchemaContext::new();
    let right = emit_rel(&cross.right, &mut right_ctx)?;

    ctx.merge(left_ctx);
    ctx.merge(right_ctx);

    Ok(proto::Rel {
        rel_type: Some(RelType::Cross(Box::new(proto::CrossRel {
            left: Some(Box::new(left)),
            right: Some(Box::new(right)),
            common: None,
            advanced_extension: None,
        }))),
    })
}

/// Emit a FilterRel
fn emit_filter(filter: &Filter, ctx: &mut SchemaContext) -> Result<proto::Rel, EmitError> {
    let input = emit_rel(&filter.input, ctx)?;
    let condition = emit_expr(&filter.predicate, ctx)?;

    Ok(proto::Rel {
        rel_type: Some(RelType::Filter(Box::new(proto::FilterRel {
            input: Some(Box::new(input)),
            condition: Some(Box::new(condition)),
            ..Default::default()
        }))),
    })
}

/// Emit an AggregateRel
fn emit_aggregate(agg: &Aggregate, ctx: &mut SchemaContext) -> Result<proto::Rel, EmitError> {
    // First emit the input to build the schema context
    let input = emit_rel(&agg.input, ctx)?;

    // GROUP BY expressions - get indices for each column
    let grouping_expressions: Vec<proto::Expression> = agg.group_by
        .iter()
        .map(|col| emit_column_ref(col, ctx))
        .collect::<Result<Vec<_>, _>>()?;

    #[allow(deprecated)]
    let groupings = if grouping_expressions.is_empty() {
        vec![]
    } else {
        vec![Grouping {
            grouping_expressions: vec![], // deprecated but required for now
            expression_references: (0..grouping_expressions.len() as u32).collect(),
        }]
    };

    // Aggregate measures
    let measures: Vec<Measure> = agg.aggregates
        .iter()
        .map(|agg_expr| emit_measure(agg_expr, ctx))
        .collect::<Result<Vec<_>, _>>()?;

    // Update context to reflect aggregate output schema:
    // GROUP BY columns + aggregate aliases
    let mut new_ctx = SchemaContext::new();
    for col in &agg.group_by {
        new_ctx.columns.push((col.table.clone(), col.name.clone()));
    }
    for agg_expr in &agg.aggregates {
        new_ctx.columns.push(("".to_string(), agg_expr.alias.clone()));
    }
    *ctx = new_ctx;

    Ok(proto::Rel {
        rel_type: Some(RelType::Aggregate(Box::new(proto::AggregateRel {
            input: Some(Box::new(input)),
            groupings,
            grouping_expressions,
            measures,
            ..Default::default()
        }))),
    })
}

/// Emit a Project relation for computed expressions (metrics)
///
/// In Substrait, ProjectRel appends expressions to the input schema.
/// We use the `emit` field to select only the columns we want in the output.
fn emit_project(proj: &Project, ctx: &mut SchemaContext) -> Result<proto::Rel, EmitError> {
    // First emit the input
    let input = emit_rel(&proj.input, ctx)?;

    // Remember input column count - ProjectRel output = input + projections
    let input_col_count = ctx.columns.len();

    // Build expressions for the projection
    let expressions: Result<Vec<proto::Expression>, EmitError> = proj.expressions
        .iter()
        .map(|proj_expr| emit_expr(&proj_expr.expr, ctx))
        .collect();
    let expressions = expressions?;

    // Build output mapping to select only the projected columns
    // The projected expressions come after input columns, so indices are:
    // input_col_count, input_col_count+1, ..., input_col_count+proj_count-1
    let output_mapping: Vec<i32> = (0..proj.expressions.len())
        .map(|i| (input_col_count + i) as i32)
        .collect();

    // Update schema context with only the projected column names
    let new_columns: Vec<(String, String)> = proj.expressions
        .iter()
        .map(|p| (String::new(), p.alias.clone()))
        .collect();
    *ctx = SchemaContext { columns: new_columns };

    Ok(proto::Rel {
        rel_type: Some(RelType::Project(Box::new(proto::ProjectRel {
            input: Some(Box::new(input)),
            expressions,
            common: Some(proto::RelCommon {
                emit_kind: Some(EmitKind::Emit(proto::rel_common::Emit { output_mapping })),
                ..Default::default()
            }),
            advanced_extension: None,
        }))),
    })
}

/// Emit a Sort relation (ORDER BY)
fn emit_sort(sort: &Sort, ctx: &mut SchemaContext) -> Result<proto::Rel, EmitError> {
    // First emit the input
    let input = emit_rel(&sort.input, ctx)?;

    // Build sort fields
    let sorts: Vec<proto::SortField> = sort.sort_keys
        .iter()
        .map(|key| {
            // Find the column index in the schema context
            let col_idx = ctx.columns
                .iter()
                .position(|(_, name)| name == &key.column)
                .unwrap_or(0) as u32;

            let direction = match key.direction {
                SortDirection::Ascending => proto::sort_field::SortDirection::AscNullsLast as i32,
                SortDirection::Descending => proto::sort_field::SortDirection::DescNullsLast as i32,
            };

            proto::SortField {
                expr: Some(emit_field_reference(col_idx).unwrap()),
                sort_kind: Some(proto::sort_field::SortKind::Direction(direction)),
            }
        })
        .collect();

    Ok(proto::Rel {
        rel_type: Some(RelType::Sort(Box::new(proto::SortRel {
            input: Some(Box::new(input)),
            sorts,
            common: None,
            advanced_extension: None,
        }))),
    })
}

/// Emit a Union relation (UNION ALL)
///
/// Combines multiple input relations with compatible schemas.
/// Used for cross-tableGroup queries and partitioned table queries.
fn emit_union(union: &Union, ctx: &mut SchemaContext) -> Result<proto::Rel, EmitError> {
    if union.inputs.len() < 2 {
        return Err(EmitError::InvalidPlan(
            "Union requires at least 2 inputs".to_string()
        ));
    }

        // Emit all inputs, each with a fresh context
    // Each branch is independent and should not share column indices
    let mut inputs: Vec<proto::Rel> = Vec::new();
    let mut first_branch_ctx: Option<SchemaContext> = None;

    for (i, input) in union.inputs.iter().enumerate() {
        let mut branch_ctx = SchemaContext::new();
        let rel = emit_rel(input, &mut branch_ctx)?;
        inputs.push(rel);

        // Save the first branch's context - this is the output schema
        if i == 0 {
            first_branch_ctx = Some(branch_ctx);
        }
    }

    // Update the output context with the first branch's schema
    // (all branches should produce the same schema for UNION ALL)
    if let Some(first_ctx) = first_branch_ctx {
        *ctx = first_ctx;
    }

    Ok(proto::Rel {
        rel_type: Some(RelType::Set(proto::SetRel {
            common: None,
            inputs,
            op: proto::set_rel::SetOp::UnionAll as i32,
            advanced_extension: None,
        })),
    })
}

/// Emit a virtual table (VALUES clause) for metadata-only queries
fn emit_virtual_table(vt: &VirtualTable, ctx: &mut SchemaContext) -> Result<proto::Rel, EmitError> {
    // Update schema context with column info
    // For virtual tables, we use empty table name since there's no physical table
    ctx.add_scan("", &vt.columns);

    // Build the schema (column types)
    let types: Vec<proto::Type> = vt.column_types.iter()
        .map(|t| type_to_substrait(t))
        .collect();

    // Build expressions for each row
    // Each row is a struct containing all column values
    let expressions: Vec<proto::expression::nested::Struct> = vt.rows.iter()
        .map(|row| {
            let fields: Vec<proto::Expression> = row.iter()
                .map(literal_value_to_expression)
                .collect();
            proto::expression::nested::Struct { fields }
        })
        .collect();

    // Create the VirtualTable read relation
    Ok(proto::Rel {
        rel_type: Some(RelType::Read(Box::new(proto::ReadRel {
            common: None,
            base_schema: Some(proto::NamedStruct {
                names: vt.columns.clone(),
                r#struct: Some(proto::r#type::Struct {
                    types,
                    type_variation_reference: 0,
                    nullability: Nullability::Required as i32,
                }),
            }),
            filter: None,
            best_effort_filter: None,
            projection: None,
            advanced_extension: None,
            read_type: Some(ReadType::VirtualTable(proto::read_rel::VirtualTable {
                expressions,
                ..Default::default()
            })),
        }))),
    })
}

/// Convert a LiteralValue to a Substrait Expression
fn literal_value_to_expression(val: &LiteralValue) -> proto::Expression {
    let literal_type = match val {
        LiteralValue::String(s) => LiteralType::String(s.clone()),
        LiteralValue::Int32(i) => LiteralType::I32(*i),
        LiteralValue::Int64(i) => LiteralType::I64(*i),
        LiteralValue::Float64(f) => LiteralType::Fp64(*f),
        LiteralValue::Bool(b) => LiteralType::Boolean(*b),
        LiteralValue::Null => LiteralType::Null(proto::Type {
            kind: Some(Kind::Bool(proto::r#type::Boolean {
                type_variation_reference: 0,
                nullability: Nullability::Nullable as i32,
            })),
        }),
    };

    proto::Expression {
        rex_type: Some(expression::RexType::Literal(proto::expression::Literal {
            nullable: true,
            type_variation_reference: 0,
            literal_type: Some(literal_type),
        })),
    }
}

/// Emit an aggregate measure
fn emit_measure(agg_expr: &AggregateExpr, ctx: &SchemaContext) -> Result<Measure, EmitError> {
    // Function reference - must match anchors in build_extensions()
    let (function_reference, invocation) = match agg_expr.func {
        Aggregation::Sum => (FUNC_SUM, AggregationInvocation::All),
        Aggregation::Avg => (FUNC_AVG, AggregationInvocation::All),
        Aggregation::Count => (FUNC_COUNT, AggregationInvocation::All),
        Aggregation::CountDistinct => (FUNC_COUNT_DISTINCT, AggregationInvocation::Distinct),
        Aggregation::Min => (FUNC_MIN, AggregationInvocation::All),
        Aggregation::Max => (FUNC_MAX, AggregationInvocation::All),
    };

    let arg_expr = emit_expr(&agg_expr.expr, ctx)?;

    Ok(Measure {
        measure: Some(proto::AggregateFunction {
            function_reference,
            arguments: vec![proto::FunctionArgument {
                arg_type: Some(ArgType::Value(arg_expr)),
            }],
            output_type: None,
            phase: AggregationPhase::Unspecified as i32,
            invocation: invocation as i32,
            ..Default::default()
        }),
        filter: None,
    })
}

/// Emit an Expression with schema context
fn emit_expr(expr: &Expr, ctx: &SchemaContext) -> Result<proto::Expression, EmitError> {
    match expr {
        Expr::Column(col) => emit_column_ref(col, ctx),
        Expr::Literal(lit) => emit_literal(lit),
        Expr::BinaryOp { left, op, right } => emit_binary_expr(left, *op, right, ctx),
        Expr::And(exprs) => emit_and(exprs, ctx),
        Expr::In { expr, values } => emit_in(expr, values, ctx),
        Expr::Sql(sql) => {
            // For simple column names (no spaces/parens), try to resolve as column
            let sql = sql.trim();
            if !sql.contains(' ') && !sql.contains('(') && sql != "*" {
                // Try to find this column in any table
                for (i, (_, col_name)) in ctx.columns.iter().enumerate() {
                    if col_name == sql {
                        return emit_field_reference(i as u32);
                    }
                }
            }
            // Fall back to placeholder for complex SQL
            Err(EmitError::UnsupportedExpression(format!("SQL expression: {}", sql)))
        }
        Expr::Add(left, right) => emit_arithmetic_expr(left, right, FUNC_ADD, ctx),
        Expr::Subtract(left, right) => emit_arithmetic_expr(left, right, FUNC_SUBTRACT, ctx),
        Expr::Multiply(left, right) => emit_arithmetic_expr(left, right, FUNC_MULTIPLY, ctx),
        Expr::Divide(left, right) => emit_divide_expr(left, right, ctx),
        Expr::Or(exprs) => emit_or(exprs, ctx),
        Expr::IsNull(inner) => emit_is_null(inner, ctx),
        Expr::IsNotNull(inner) => emit_is_not_null(inner, ctx),
        Expr::Case { when_then, else_result } => emit_case(when_then, else_result.as_deref(), ctx),
        Expr::Coalesce(exprs) => emit_coalesce(exprs, ctx),
    }
}

/// Emit a column reference using schema context
fn emit_column_ref(col: &Column, ctx: &SchemaContext) -> Result<proto::Expression, EmitError> {
    // Handle unqualified column references (e.g., from metrics referencing measures)
    if col.table.is_empty() {
        // Search by column name only
        for (i, (_, c)) in ctx.columns.iter().enumerate() {
            if c == &col.name {
                return emit_field_reference(i as u32);
            }
        }
        return Err(EmitError::ColumnNotFound(col.name.clone()));
    }

    let idx = ctx.find_column(&col.table, &col.name)
        .ok_or_else(|| EmitError::ColumnNotFound(col.qualified_name()))?;

    emit_field_reference(idx as u32)
}

/// Emit a field reference by index
fn emit_field_reference(field: u32) -> Result<proto::Expression, EmitError> {
    Ok(proto::Expression {
        rex_type: Some(expression::RexType::Selection(Box::new(
            proto::expression::FieldReference {
                reference_type: Some(expression::field_reference::ReferenceType::DirectReference(
                    ReferenceSegment {
                        reference_type: Some(ReferenceType::StructField(Box::new(StructField {
                            field: field as i32,
                            child: None,
                        }))),
                    }
                )),
                root_type: None,
            }
        ))),
    })
}

/// Emit a literal value
fn emit_literal(lit: &Literal) -> Result<proto::Expression, EmitError> {
    let literal_type = match lit {
        Literal::Null(type_name) => {
            LiteralType::Null(type_to_substrait(type_name))
        }
        Literal::Bool(b) => LiteralType::Boolean(*b),
        Literal::Int(i) => LiteralType::I64(*i),
        Literal::Float(f) => LiteralType::Fp64(*f),
        Literal::String(s) => LiteralType::String(s.clone()),
    };

    Ok(proto::Expression {
        rex_type: Some(expression::RexType::Literal(proto::expression::Literal {
            nullable: true,
            type_variation_reference: 0,
            literal_type: Some(literal_type),
        })),
    })
}

/// Emit a binary expression with column indices (for join conditions)
fn emit_binary_expr_with_indices(left_idx: u32, op: BinaryOperator, right_idx: u32) -> proto::Expression {
    let left_expr = emit_field_reference(left_idx).unwrap();
    let right_expr = emit_field_reference(right_idx).unwrap();

    let function_reference = comparison_function_ref(op);

    proto::Expression {
        rex_type: Some(expression::RexType::ScalarFunction(
            proto::expression::ScalarFunction {
                function_reference,
                arguments: vec![
                    proto::FunctionArgument {
                        arg_type: Some(ArgType::Value(left_expr)),
                    },
                    proto::FunctionArgument {
                        arg_type: Some(ArgType::Value(right_expr)),
                    },
                ],
                output_type: None,
                ..Default::default()
            }
        )),
    }
}

/// Build an AND expression from two already-built proto Expressions.
fn emit_and_expr(left: proto::Expression, right: proto::Expression) -> proto::Expression {
    proto::Expression {
        rex_type: Some(expression::RexType::ScalarFunction(
            proto::expression::ScalarFunction {
                function_reference: FUNC_AND,
                arguments: vec![
                    proto::FunctionArgument {
                        arg_type: Some(ArgType::Value(left)),
                    },
                    proto::FunctionArgument {
                        arg_type: Some(ArgType::Value(right)),
                    },
                ],
                output_type: None,
                ..Default::default()
            }
        )),
    }
}

/// Get the function reference for a comparison operator
fn comparison_function_ref(op: BinaryOperator) -> u32 {
    match op {
        BinaryOperator::Eq => FUNC_EQUAL,
        BinaryOperator::NotEq => FUNC_NOT_EQUAL,
        BinaryOperator::Lt => FUNC_LT,
        BinaryOperator::LtEq => FUNC_LTE,
        BinaryOperator::Gt => FUNC_GT,
        BinaryOperator::GtEq => FUNC_GTE,
    }
}

/// Emit a binary expression
fn emit_binary_expr(
    left: &Expr,
    op: BinaryOperator,
    right: &Expr,
    ctx: &SchemaContext,
) -> Result<proto::Expression, EmitError> {
    let left_expr = emit_expr(left, ctx)?;
    let right_expr = emit_expr(right, ctx)?;

    let function_reference = comparison_function_ref(op);

    Ok(proto::Expression {
        rex_type: Some(expression::RexType::ScalarFunction(
            proto::expression::ScalarFunction {
                function_reference,
                arguments: vec![
                    proto::FunctionArgument {
                        arg_type: Some(ArgType::Value(left_expr)),
                    },
                    proto::FunctionArgument {
                        arg_type: Some(ArgType::Value(right_expr)),
                    },
                ],
                output_type: None,
                ..Default::default()
            }
        )),
    })
}

/// Emit an AND expression
fn emit_and(exprs: &[Expr], ctx: &SchemaContext) -> Result<proto::Expression, EmitError> {
    if exprs.is_empty() {
        return emit_literal(&Literal::Bool(true));
    }
    if exprs.len() == 1 {
        return emit_expr(&exprs[0], ctx);
    }

    let args: Vec<proto::FunctionArgument> = exprs
        .iter()
        .map(|e| {
            let expr = emit_expr(e, ctx)?;
            Ok(proto::FunctionArgument {
                arg_type: Some(ArgType::Value(expr)),
            })
        })
        .collect::<Result<Vec<_>, EmitError>>()?;

    Ok(proto::Expression {
        rex_type: Some(expression::RexType::ScalarFunction(
            proto::expression::ScalarFunction {
                function_reference: FUNC_AND,
                arguments: args,
                output_type: None,
                ..Default::default()
            }
        )),
    })
}

/// Emit an OR expression
fn emit_or(exprs: &[Expr], ctx: &SchemaContext) -> Result<proto::Expression, EmitError> {
    if exprs.is_empty() {
        return emit_literal(&Literal::Bool(false));
    }
    if exprs.len() == 1 {
        return emit_expr(&exprs[0], ctx);
    }

    let args: Vec<proto::FunctionArgument> = exprs
        .iter()
        .map(|e| {
            let expr = emit_expr(e, ctx)?;
            Ok(proto::FunctionArgument {
                arg_type: Some(ArgType::Value(expr)),
            })
        })
        .collect::<Result<Vec<_>, EmitError>>()?;

    Ok(proto::Expression {
        rex_type: Some(expression::RexType::ScalarFunction(
            proto::expression::ScalarFunction {
                function_reference: FUNC_OR,
                arguments: args,
                output_type: None,
                ..Default::default()
            }
        )),
    })
}

/// Emit an IS NULL expression
fn emit_is_null(inner: &Expr, ctx: &SchemaContext) -> Result<proto::Expression, EmitError> {
    let inner_expr = emit_expr(inner, ctx)?;

    Ok(proto::Expression {
        rex_type: Some(expression::RexType::ScalarFunction(
            proto::expression::ScalarFunction {
                function_reference: FUNC_IS_NULL,
                arguments: vec![
                    proto::FunctionArgument {
                        arg_type: Some(ArgType::Value(inner_expr)),
                    },
                ],
                output_type: None,
                ..Default::default()
            }
        )),
    })
}

/// Emit an IS NOT NULL expression
fn emit_is_not_null(inner: &Expr, ctx: &SchemaContext) -> Result<proto::Expression, EmitError> {
    let inner_expr = emit_expr(inner, ctx)?;

    Ok(proto::Expression {
        rex_type: Some(expression::RexType::ScalarFunction(
            proto::expression::ScalarFunction {
                function_reference: FUNC_IS_NOT_NULL,
                arguments: vec![
                    proto::FunctionArgument {
                        arg_type: Some(ArgType::Value(inner_expr)),
                    },
                ],
                output_type: None,
                ..Default::default()
            }
        )),
    })
}

/// Emit a COALESCE expression - returns first non-NULL value
fn emit_coalesce(exprs: &[Expr], ctx: &SchemaContext) -> Result<proto::Expression, EmitError> {
    let arguments: Vec<proto::FunctionArgument> = exprs
        .iter()
        .map(|e| {
            emit_expr(e, ctx).map(|expr| proto::FunctionArgument {
                arg_type: Some(ArgType::Value(expr)),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(proto::Expression {
        rex_type: Some(expression::RexType::ScalarFunction(
            proto::expression::ScalarFunction {
                function_reference: FUNC_COALESCE,
                arguments,
                output_type: None,
                ..Default::default()
            }
        )),
    })
}

/// Emit a CASE WHEN expression
fn emit_case(
    when_then: &[(Expr, Expr)],
    else_result: Option<&Expr>,
    ctx: &SchemaContext,
) -> Result<proto::Expression, EmitError> {
    let ifs: Vec<expression::if_then::IfClause> = when_then
        .iter()
        .map(|(cond, then)| {
            let r#if = emit_expr(cond, ctx)?;
            let then = emit_expr(then, ctx)?;
            Ok(expression::if_then::IfClause {
                r#if: Some(r#if),
                then: Some(then),
            })
        })
        .collect::<Result<Vec<_>, EmitError>>()?;

    let else_expr = match else_result {
        Some(e) => Some(Box::new(emit_expr(e, ctx)?)),
        None => None,
    };

    Ok(proto::Expression {
        rex_type: Some(expression::RexType::IfThen(Box::new(
            expression::IfThen {
                ifs,
                r#else: else_expr,
            }
        ))),
    })
}

/// Emit an arithmetic expression (add, subtract, multiply)
fn emit_arithmetic_expr(
    left: &Expr,
    right: &Expr,
    function_reference: u32,
    ctx: &SchemaContext,
) -> Result<proto::Expression, EmitError> {
    let left_expr = emit_expr(left, ctx)?;
    let right_expr = emit_expr(right, ctx)?;

    Ok(proto::Expression {
        rex_type: Some(expression::RexType::ScalarFunction(
            proto::expression::ScalarFunction {
                function_reference,
                arguments: vec![
                    proto::FunctionArgument {
                        arg_type: Some(ArgType::Value(left_expr)),
                    },
                    proto::FunctionArgument {
                        arg_type: Some(ArgType::Value(right_expr)),
                    },
                ],
                output_type: None,
                ..Default::default()
            }
        )),
    })
}

/// Emit a divide expression, casting operands to f64 for float division
fn emit_divide_expr(
    left: &Expr,
    right: &Expr,
    ctx: &SchemaContext,
) -> Result<proto::Expression, EmitError> {
    let left_expr = emit_cast_to_f64(emit_expr(left, ctx)?);
    let right_expr = emit_cast_to_f64(emit_expr(right, ctx)?);

    Ok(proto::Expression {
        rex_type: Some(expression::RexType::ScalarFunction(
            proto::expression::ScalarFunction {
                function_reference: FUNC_DIVIDE,
                arguments: vec![
                    proto::FunctionArgument {
                        arg_type: Some(ArgType::Value(left_expr)),
                    },
                    proto::FunctionArgument {
                        arg_type: Some(ArgType::Value(right_expr)),
                    },
                ],
                output_type: None,
                ..Default::default()
            }
        )),
    })
}

/// Wrap an expression in a CAST to f64 for float division
fn emit_cast_to_f64(expr: proto::Expression) -> proto::Expression {
    proto::Expression {
        rex_type: Some(expression::RexType::Cast(Box::new(
            proto::expression::Cast {
                r#type: Some(proto::Type {
                    kind: Some(Kind::Fp64(proto::r#type::Fp64 {
                        type_variation_reference: 0,
                        nullability: Nullability::Nullable as i32,
                    })),
                }),
                input: Some(Box::new(expr)),
                failure_behavior: 0,
            }
        ))),
    }
}

/// Emit an IN expression
fn emit_in(expr: &Expr, values: &[Expr], ctx: &SchemaContext) -> Result<proto::Expression, EmitError> {
    let needle = emit_expr(expr, ctx)?;
    let haystack: Vec<proto::Expression> = values
        .iter()
        .map(|v| emit_expr(v, ctx))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(proto::Expression {
        rex_type: Some(expression::RexType::SingularOrList(Box::new(
            proto::expression::SingularOrList {
                value: Some(Box::new(needle)),
                options: haystack,
            }
        ))),
    })
}
