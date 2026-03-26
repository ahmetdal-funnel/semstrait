//! SubstraitSerializer - convert LogicalPlan to/from Substrait Plan

use crate::error::{DeserializeError, SerializeError};
use crate::plan::{LogicalPlan, NodeMeta};
use crate::plan::node::*;
use crate::schema::{Field, Schema};
use super::anchors::*;
use super::expr_converter::ExprConverter;
use semstrait_core::DataType;
use substrait::proto::{
    self,
    aggregate_function::AggregationInvocation,
    aggregate_rel::{Grouping, Measure},
    extensions::{
        simple_extension_declaration::MappingType, SimpleExtensionDeclaration, SimpleExtensionUri,
    },
    function_argument::ArgType,
    plan_rel::RelType as PlanRelType,
    r#type::{Kind, Nullability},
    read_rel::{NamedTable, ReadType},
    rel::RelType,
    rel_common::EmitKind,
    AggregationPhase,
};

/// Serializes LogicalPlan to/from Substrait
pub struct SubstraitSerializer;

impl SubstraitSerializer {
    /// Serialize a LogicalPlan to Substrait Plan
    #[allow(deprecated)]
    pub fn to_substrait(plan: &LogicalPlan) -> Result<proto::Plan, SerializeError> {
        let root_rel = Self::node_to_rel(&plan.root)?;

        Ok(proto::Plan {
            version: Some(proto::Version {
                major_number: 0,
                minor_number: 62,
                patch_number: 0,
                git_hash: String::new(),
                producer: String::new(),
            }),
            extension_uris: Self::build_extension_uris(),
            extensions: Self::build_extensions(),
            extension_urns: vec![],
            relations: vec![proto::PlanRel {
                rel_type: Some(PlanRelType::Root(proto::RelRoot {
                    input: Some(root_rel),
                    names: plan.output_names.clone(),
                })),
            }],
            advanced_extensions: None,
            expected_type_urls: vec![],
            parameter_bindings: vec![],
            type_aliases: vec![],
        })
    }

    /// Deserialize a Substrait Plan to LogicalPlan
    pub fn from_substrait(plan: &proto::Plan) -> Result<LogicalPlan, DeserializeError> {
        if plan.relations.is_empty() {
            return Err(DeserializeError::InvalidPlan(
                "Plan has no relations".to_string(),
            ));
        }

        let plan_rel = &plan.relations[0];
        let root_rel = match &plan_rel.rel_type {
            Some(PlanRelType::Root(root)) => root,
            _ => {
                return Err(DeserializeError::InvalidPlan(
                    "First relation is not a RelRoot".to_string(),
                ))
            }
        };

        let input = root_rel
            .input
            .as_ref()
            .ok_or_else(|| DeserializeError::MissingField("root input".to_string()))?;

        let root_node = Self::rel_to_node(input)?;
        let output_names = root_rel.names.clone();

        Ok(LogicalPlan::new(root_node, output_names))
    }

    fn node_to_rel(node: &PlanNode) -> Result<proto::Rel, SerializeError> {
        match node {
            PlanNode::Scan(n) => Self::scan_to_rel(n),
            PlanNode::Filter(n) => Self::filter_to_rel(n),
            PlanNode::Project(n) => Self::project_to_rel(n),
            PlanNode::Aggregate(n) => Self::aggregate_to_rel(n),
            PlanNode::Join(n) => Self::join_to_rel(n),
            PlanNode::Union(n) => Self::union_to_rel(n),
            PlanNode::Sort(n) => Self::sort_to_rel(n),
            PlanNode::Fetch(n) => Self::fetch_to_rel(n),
        }
    }

    fn rel_to_node(rel: &proto::Rel) -> Result<PlanNode, DeserializeError> {
        match &rel.rel_type {
            Some(RelType::Read(read)) => Self::rel_to_scan(read),
            Some(RelType::Filter(filter)) => Self::rel_to_filter(filter),
            Some(RelType::Project(project)) => Self::rel_to_project(project),
            Some(RelType::Aggregate(agg)) => Self::rel_to_aggregate(agg),
            Some(RelType::Join(join)) => Self::rel_to_join(join),
            Some(RelType::Set(set)) => Self::rel_to_union(set),
            Some(RelType::Sort(sort)) => Self::rel_to_sort(sort),
            Some(RelType::Fetch(fetch)) => Self::rel_to_fetch(fetch),
            _ => Err(DeserializeError::UnsupportedConstruct(
                "Unsupported relation type".to_string(),
            )),
        }
    }

    fn scan_to_rel(node: &ScanNode) -> Result<proto::Rel, SerializeError> {
        let table_names: Vec<String> = node.table_name.split('.').map(String::from).collect();

        let types: Vec<proto::Type> = node
            .meta
            .output_schema
            .fields
            .iter()
            .map(|f| Self::datatype_to_substrait(&f.data_type))
            .collect();

        let base_schema = proto::NamedStruct {
            names: node.projection.clone(),
            r#struct: Some(proto::r#type::Struct {
                types,
                type_variation_reference: 0,
                nullability: Nullability::Nullable as i32,
            }),
        };

        Ok(proto::Rel {
            rel_type: Some(RelType::Read(Box::new(proto::ReadRel {
                common: None,
                base_schema: Some(base_schema),
                filter: None,
                best_effort_filter: None,
                projection: None,
                advanced_extension: None,
                read_type: Some(ReadType::NamedTable(NamedTable {
                    names: table_names,
                    advanced_extension: None,
                })),
            }))),
        })
    }

    fn rel_to_scan(read: &proto::ReadRel) -> Result<PlanNode, DeserializeError> {
        let table_name = match &read.read_type {
            Some(ReadType::NamedTable(nt)) => nt.names.join("."),
            _ => {
                return Err(DeserializeError::UnsupportedConstruct(
                    "Only NamedTable scans supported".to_string(),
                ))
            }
        };

        let base_schema = read
            .base_schema
            .as_ref()
            .ok_or_else(|| DeserializeError::MissingField("base_schema".to_string()))?;

        let projection = base_schema.names.clone();
        let empty_types = vec![];
        let types = base_schema
            .r#struct
            .as_ref()
            .map(|s| &s.types)
            .unwrap_or(&empty_types);

        let fields: Vec<Field> = projection
            .iter()
            .zip(types.iter())
            .map(|(name, typ)| {
                let data_type = Self::substrait_to_datatype(typ);
                Field::new(name.clone(), data_type)
            })
            .collect();

        let schema = Schema::new(fields);
        let meta = NodeMeta::new(schema);

        Ok(PlanNode::Scan(ScanNode {
            meta,
            table_name,
            location: None,
            format: None,
            projection,
        }))
    }

    fn filter_to_rel(node: &FilterNode) -> Result<proto::Rel, SerializeError> {
        let input = Self::node_to_rel(&node.input)?;
        let converter = ExprConverter::new(&node.input.meta().output_schema);
        let condition = converter.to_substrait(&node.predicate)?;

        Ok(proto::Rel {
            rel_type: Some(RelType::Filter(Box::new(proto::FilterRel {
                common: None,
                input: Some(Box::new(input)),
                condition: Some(Box::new(condition)),
                advanced_extension: None,
            }))),
        })
    }

    fn rel_to_filter(filter: &proto::FilterRel) -> Result<PlanNode, DeserializeError> {
        let input_rel = filter
            .input
            .as_ref()
            .ok_or_else(|| DeserializeError::MissingField("filter input".to_string()))?;
        let input = Self::rel_to_node(input_rel)?;

        let condition = filter
            .condition
            .as_ref()
            .ok_or_else(|| DeserializeError::MissingField("filter condition".to_string()))?;

        let converter = ExprConverter::new(&input.meta().output_schema);
        let predicate = converter.from_substrait(condition)?;

        let meta = NodeMeta::new(input.meta().output_schema.clone());

        Ok(PlanNode::Filter(FilterNode {
            meta,
            input: Box::new(input),
            predicate,
        }))
    }

    fn project_to_rel(node: &ProjectNode) -> Result<proto::Rel, SerializeError> {
        let input = Self::node_to_rel(&node.input)?;
        let input_schema = &node.input.meta().output_schema;
        let converter = ExprConverter::new(input_schema);

        let expressions: Result<Vec<_>, _> = node
            .expressions
            .iter()
            .map(|e| converter.to_substrait(e))
            .collect();
        let expressions = expressions?;

        let input_col_count = input_schema.len();
        let output_mapping: Vec<i32> = (0..node.expressions.len())
            .map(|i| (input_col_count + i) as i32)
            .collect();

        Ok(proto::Rel {
            rel_type: Some(RelType::Project(Box::new(proto::ProjectRel {
                common: Some(proto::RelCommon {
                    hint: None,
                    advanced_extension: None,
                    emit_kind: Some(EmitKind::Emit(proto::rel_common::Emit {
                        output_mapping,
                    })),
                }),
                input: Some(Box::new(input)),
                expressions,
                advanced_extension: None,
            }))),
        })
    }

    fn rel_to_project(project: &proto::ProjectRel) -> Result<PlanNode, DeserializeError> {
        let input_rel = project
            .input
            .as_ref()
            .ok_or_else(|| DeserializeError::MissingField("project input".to_string()))?;
        let input = Self::rel_to_node(input_rel)?;

        let converter = ExprConverter::new(&input.meta().output_schema);
        let expressions: Result<Vec<_>, _> = project
            .expressions
            .iter()
            .map(|e| converter.from_substrait(e))
            .collect();
        let expressions = expressions?;

        let fields: Vec<Field> = (0..expressions.len())
            .map(|i| Field::new(format!("expr_{}", i), DataType::Float64))
            .collect();
        let schema = Schema::new(fields);
        let meta = NodeMeta::new(schema);

        Ok(PlanNode::Project(ProjectNode {
            meta,
            input: Box::new(input),
            expressions,
        }))
    }

    #[allow(deprecated)]
    fn aggregate_to_rel(node: &AggNode) -> Result<proto::Rel, SerializeError> {
        let input = Self::node_to_rel(&node.input)?;
        let input_schema = &node.input.meta().output_schema;
        let converter = ExprConverter::new(input_schema);

        let grouping_expressions: Result<Vec<_>, _> = node
            .group_by
            .iter()
            .map(|e| converter.to_substrait(e))
            .collect();
        let grouping_expressions = grouping_expressions?;

        let groupings = if grouping_expressions.is_empty() {
            vec![]
        } else {
            vec![Grouping {
                grouping_expressions: vec![],
                expression_references: (0..grouping_expressions.len() as u32).collect(),
            }]
        };

        let measures: Result<Vec<_>, _> = node
            .aggregates
            .iter()
            .map(|m| Self::measure_to_substrait(m, &converter))
            .collect();
        let measures = measures?;

        Ok(proto::Rel {
            rel_type: Some(RelType::Aggregate(Box::new(proto::AggregateRel {
                common: None,
                input: Some(Box::new(input)),
                groupings,
                grouping_expressions,
                measures,
                advanced_extension: None,
            }))),
        })
    }

    fn rel_to_aggregate(agg: &proto::AggregateRel) -> Result<PlanNode, DeserializeError> {
        let input_rel = agg
            .input
            .as_ref()
            .ok_or_else(|| DeserializeError::MissingField("aggregate input".to_string()))?;
        let input = Self::rel_to_node(input_rel)?;

        let converter = ExprConverter::new(&input.meta().output_schema);

        let group_by: Result<Vec<_>, _> = agg
            .grouping_expressions
            .iter()
            .map(|e| converter.from_substrait(e))
            .collect();
        let group_by = group_by?;

        let aggregates: Result<Vec<_>, _> = agg
            .measures
            .iter()
            .map(|m| Self::substrait_to_measure(m, &converter))
            .collect();
        let aggregates = aggregates?;

        let mut fields = Vec::new();
        for (i, _) in group_by.iter().enumerate() {
            fields.push(Field::new(format!("group_{}", i), DataType::Utf8));
        }
        for (i, _) in aggregates.iter().enumerate() {
            fields.push(Field::new(format!("agg_{}", i), DataType::Float64));
        }

        let schema = Schema::new(fields);
        let meta = NodeMeta::new(schema);

        Ok(PlanNode::Aggregate(AggNode {
            meta,
            input: Box::new(input),
            group_by,
            aggregates,
        }))
    }

    fn measure_to_substrait(
        measure: &AggregateMeasure,
        converter: &ExprConverter,
    ) -> Result<Measure, SerializeError> {
        let (function_reference, invocation) = match measure.function {
            Aggregation::Sum => (FUNC_SUM, AggregationInvocation::All),
            Aggregation::Avg => (FUNC_AVG, AggregationInvocation::All),
            Aggregation::Count => (FUNC_COUNT, AggregationInvocation::All),
            Aggregation::CountDistinct => (FUNC_COUNT_DISTINCT, AggregationInvocation::Distinct),
            Aggregation::Min => (FUNC_MIN, AggregationInvocation::All),
            Aggregation::Max => (FUNC_MAX, AggregationInvocation::All),
        };

        let arg_expr = converter.to_substrait(&measure.expr)?;

        Ok(Measure {
            measure: Some(proto::AggregateFunction {
                function_reference,
                arguments: vec![proto::FunctionArgument {
                    arg_type: Some(ArgType::Value(arg_expr)),
                }],
                #[allow(deprecated)]
                args: vec![],
                sorts: vec![],
                output_type: None,
                phase: AggregationPhase::Unspecified as i32,
                invocation: invocation as i32,
                options: vec![],
            }),
            filter: None,
        })
    }

    fn substrait_to_measure(
        measure: &Measure,
        converter: &ExprConverter,
    ) -> Result<AggregateMeasure, DeserializeError> {
        let agg_func = measure
            .measure
            .as_ref()
            .ok_or_else(|| DeserializeError::MissingField("measure function".to_string()))?;

        let function = match agg_func.function_reference {
            FUNC_SUM => Aggregation::Sum,
            FUNC_AVG => Aggregation::Avg,
            FUNC_COUNT => Aggregation::Count,
            FUNC_COUNT_DISTINCT => Aggregation::CountDistinct,
            FUNC_MIN => Aggregation::Min,
            FUNC_MAX => Aggregation::Max,
            _ => {
                return Err(DeserializeError::UnsupportedConstruct(format!(
                    "Unknown aggregate function: {}",
                    agg_func.function_reference
                )))
            }
        };

        let distinct = agg_func.invocation == AggregationInvocation::Distinct as i32;

        let arg = agg_func
            .arguments
            .first()
            .and_then(|a| match &a.arg_type {
                Some(ArgType::Value(e)) => Some(e),
                _ => None,
            })
            .ok_or_else(|| DeserializeError::MissingField("aggregate argument".to_string()))?;

        let expr = converter.from_substrait(arg)?;

        Ok(AggregateMeasure {
            function,
            expr,
            distinct,
        })
    }

    fn join_to_rel(node: &JoinNode) -> Result<proto::Rel, SerializeError> {
        let left = Self::node_to_rel(&node.left)?;
        let right = Self::node_to_rel(&node.right)?;

        let join_type = match node.join_type {
            JoinType::Inner => proto::join_rel::JoinType::Inner,
            JoinType::Left => proto::join_rel::JoinType::Left,
            JoinType::Right => proto::join_rel::JoinType::Right,
            JoinType::Full => proto::join_rel::JoinType::Outer,
        };

        let mut combined_fields = node.left.meta().output_schema.fields.clone();
        combined_fields.extend(node.right.meta().output_schema.fields.clone());
        let combined_schema = Schema::new(combined_fields);
        let converter = ExprConverter::new(&combined_schema);

        let expression = converter.to_substrait(&node.condition)?;

        Ok(proto::Rel {
            rel_type: Some(RelType::Join(Box::new(proto::JoinRel {
                common: None,
                left: Some(Box::new(left)),
                right: Some(Box::new(right)),
                expression: Some(Box::new(expression)),
                post_join_filter: None,
                r#type: join_type as i32,
                advanced_extension: None,
            }))),
        })
    }

    fn rel_to_join(join: &proto::JoinRel) -> Result<PlanNode, DeserializeError> {
        let left_rel = join
            .left
            .as_ref()
            .ok_or_else(|| DeserializeError::MissingField("join left".to_string()))?;
        let left = Self::rel_to_node(left_rel)?;

        let right_rel = join
            .right
            .as_ref()
            .ok_or_else(|| DeserializeError::MissingField("join right".to_string()))?;
        let right = Self::rel_to_node(right_rel)?;

        let join_type = match proto::join_rel::JoinType::try_from(join.r#type) {
            Ok(proto::join_rel::JoinType::Inner) => JoinType::Inner,
            Ok(proto::join_rel::JoinType::Left) => JoinType::Left,
            Ok(proto::join_rel::JoinType::Right) => JoinType::Right,
            Ok(proto::join_rel::JoinType::Outer) => JoinType::Full,
            _ => JoinType::Inner,
        };

        let mut combined_fields = left.meta().output_schema.fields.clone();
        combined_fields.extend(right.meta().output_schema.fields.clone());
        let combined_schema = Schema::new(combined_fields);
        let converter = ExprConverter::new(&combined_schema);

        let expr = join
            .expression
            .as_ref()
            .ok_or_else(|| DeserializeError::MissingField("join expression".to_string()))?;
        let condition = converter.from_substrait(expr)?;

        let meta = NodeMeta::new(combined_schema);

        Ok(PlanNode::Join(JoinNode {
            meta,
            left: Box::new(left),
            right: Box::new(right),
            join_type,
            condition,
        }))
    }

    fn union_to_rel(node: &UnionNode) -> Result<proto::Rel, SerializeError> {
        let inputs: Result<Vec<_>, _> =
            node.inputs.iter().map(Self::node_to_rel).collect();
        let inputs = inputs?;

        Ok(proto::Rel {
            rel_type: Some(RelType::Set(proto::SetRel {
                common: None,
                inputs,
                op: if node.distinct {
                    proto::set_rel::SetOp::UnionDistinct as i32
                } else {
                    proto::set_rel::SetOp::UnionAll as i32
                },
                advanced_extension: None,
            })),
        })
    }

    fn rel_to_union(set: &proto::SetRel) -> Result<PlanNode, DeserializeError> {
        let inputs: Result<Vec<_>, _> =
            set.inputs.iter().map(Self::rel_to_node).collect();
        let inputs = inputs?;

        if inputs.is_empty() {
            return Err(DeserializeError::InvalidPlan(
                "Union must have at least one input".to_string(),
            ));
        }

        let schema = inputs[0].meta().output_schema.clone();
        let meta = NodeMeta::new(schema);

        let distinct = set.op == proto::set_rel::SetOp::UnionDistinct as i32;

        Ok(PlanNode::Union(UnionNode { meta, inputs, distinct }))
    }

    fn sort_to_rel(node: &SortNode) -> Result<proto::Rel, SerializeError> {
        let input = Self::node_to_rel(&node.input)?;
        let converter = ExprConverter::new(&node.input.meta().output_schema);

        let sorts: Result<Vec<proto::SortField>, SerializeError> = node
            .sort_keys
            .iter()
            .map(|key| {
                let expr = converter.to_substrait(&key.expr)?;
                let direction = match key.direction {
                    SortDirection::Ascending => {
                        proto::sort_field::SortDirection::AscNullsLast as i32
                    }
                    SortDirection::Descending => {
                        proto::sort_field::SortDirection::DescNullsLast as i32
                    }
                };

                Ok(proto::SortField {
                    expr: Some(expr),
                    sort_kind: Some(proto::sort_field::SortKind::Direction(direction)),
                })
            })
            .collect();
        let sorts = sorts?;

        Ok(proto::Rel {
            rel_type: Some(RelType::Sort(Box::new(proto::SortRel {
                common: None,
                input: Some(Box::new(input)),
                sorts,
                advanced_extension: None,
            }))),
        })
    }

    fn rel_to_sort(sort: &proto::SortRel) -> Result<PlanNode, DeserializeError> {
        let input_rel = sort
            .input
            .as_ref()
            .ok_or_else(|| DeserializeError::MissingField("sort input".to_string()))?;
        let input = Self::rel_to_node(input_rel)?;

        let converter = ExprConverter::new(&input.meta().output_schema);

        let sort_keys: Result<Vec<SortKey>, DeserializeError> = sort
            .sorts
            .iter()
            .map(|s| {
                let expr = s
                    .expr
                    .as_ref()
                    .ok_or_else(|| DeserializeError::MissingField("sort expression".to_string()))?;
                let expr = converter.from_substrait(expr)?;

                let direction = match &s.sort_kind {
                    Some(proto::sort_field::SortKind::Direction(d)) => {
                        if *d == proto::sort_field::SortDirection::AscNullsFirst as i32
                            || *d == proto::sort_field::SortDirection::AscNullsLast as i32
                        {
                            SortDirection::Ascending
                        } else {
                            SortDirection::Descending
                        }
                    }
                    _ => SortDirection::Ascending,
                };

                Ok(SortKey { expr, direction })
            })
            .collect();
        let sort_keys = sort_keys?;

        let meta = NodeMeta::new(input.meta().output_schema.clone());

        Ok(PlanNode::Sort(SortNode {
            meta,
            input: Box::new(input),
            sort_keys,
        }))
    }

    #[allow(deprecated)]
    fn fetch_to_rel(node: &FetchNode) -> Result<proto::Rel, SerializeError> {
        let input = Self::node_to_rel(&node.input)?;

        let offset_mode = if node.offset > 0 {
            Some(proto::fetch_rel::OffsetMode::Offset(node.offset))
        } else {
            None
        };

        let count_mode = node.count.map(proto::fetch_rel::CountMode::Count);

        Ok(proto::Rel {
            rel_type: Some(RelType::Fetch(Box::new(proto::FetchRel {
                common: None,
                input: Some(Box::new(input)),
                offset_mode,
                count_mode,
                advanced_extension: None,
            }))),
        })
    }

    #[allow(deprecated)]
    fn rel_to_fetch(fetch: &proto::FetchRel) -> Result<PlanNode, DeserializeError> {
        let input_rel = fetch
            .input
            .as_ref()
            .ok_or_else(|| DeserializeError::MissingField("fetch input".to_string()))?;
        let input = Self::rel_to_node(input_rel)?;

        let offset = match &fetch.offset_mode {
            Some(proto::fetch_rel::OffsetMode::Offset(o)) => *o,
            _ => 0,
        };

        let count = match &fetch.count_mode {
            Some(proto::fetch_rel::CountMode::Count(c)) => Some(*c),
            _ => None,
        };

        let meta = NodeMeta::new(input.meta().output_schema.clone());

        Ok(PlanNode::Fetch(FetchNode {
            meta,
            input: Box::new(input),
            count,
            offset,
        }))
    }

    fn build_extension_uris() -> Vec<SimpleExtensionUri> {
        vec![
            SimpleExtensionUri {
                extension_uri_anchor: URI_AGGREGATE,
                uri: "/functions_aggregate_generic.yaml".to_string(),
            },
            SimpleExtensionUri {
                extension_uri_anchor: URI_COMPARISON,
                uri: "/functions_comparison.yaml".to_string(),
            },
            SimpleExtensionUri {
                extension_uri_anchor: URI_BOOLEAN,
                uri: "/functions_boolean.yaml".to_string(),
            },
            SimpleExtensionUri {
                extension_uri_anchor: URI_ARITHMETIC,
                uri: "/functions_arithmetic.yaml".to_string(),
            },
        ]
    }

    #[allow(deprecated)]
    fn build_extensions() -> Vec<SimpleExtensionDeclaration> {
        vec![
            Self::make_function_extension(URI_AGGREGATE, FUNC_SUM, "sum"),
            Self::make_function_extension(URI_AGGREGATE, FUNC_AVG, "avg"),
            Self::make_function_extension(URI_AGGREGATE, FUNC_COUNT, "count"),
            Self::make_function_extension(URI_AGGREGATE, FUNC_COUNT_DISTINCT, "count"),
            Self::make_function_extension(URI_AGGREGATE, FUNC_MIN, "min"),
            Self::make_function_extension(URI_AGGREGATE, FUNC_MAX, "max"),
            Self::make_function_extension(URI_COMPARISON, FUNC_EQUAL, "equal"),
            Self::make_function_extension(URI_COMPARISON, FUNC_NOT_EQUAL, "not_equal"),
            Self::make_function_extension(URI_COMPARISON, FUNC_LT, "lt"),
            Self::make_function_extension(URI_COMPARISON, FUNC_LTE, "lte"),
            Self::make_function_extension(URI_COMPARISON, FUNC_GT, "gt"),
            Self::make_function_extension(URI_COMPARISON, FUNC_GTE, "gte"),
            Self::make_function_extension(URI_BOOLEAN, FUNC_AND, "and"),
            Self::make_function_extension(URI_BOOLEAN, FUNC_OR, "or"),
            Self::make_function_extension(URI_BOOLEAN, FUNC_NOT, "not"),
            Self::make_function_extension(URI_BOOLEAN, FUNC_IS_NULL, "is_null"),
            Self::make_function_extension(URI_BOOLEAN, FUNC_IS_NOT_NULL, "is_not_null"),
            Self::make_function_extension(URI_BOOLEAN, FUNC_IN, "in"),
            Self::make_function_extension(URI_BOOLEAN, FUNC_BETWEEN, "between"),
            Self::make_function_extension(URI_BOOLEAN, FUNC_LIKE, "like"),
            Self::make_function_extension(URI_BOOLEAN, FUNC_COALESCE, "coalesce"),
            Self::make_function_extension(URI_BOOLEAN, FUNC_NULLIF, "nullif"),
            Self::make_function_extension(URI_BOOLEAN, FUNC_DATE_TRUNC, "date_trunc"),
            Self::make_function_extension(URI_ARITHMETIC, FUNC_ADD, "add"),
            Self::make_function_extension(URI_ARITHMETIC, FUNC_SUBTRACT, "subtract"),
            Self::make_function_extension(URI_ARITHMETIC, FUNC_MULTIPLY, "multiply"),
            Self::make_function_extension(URI_ARITHMETIC, FUNC_DIVIDE, "divide"),
        ]
    }

    #[allow(deprecated)]
    fn make_function_extension(
        uri_ref: u32,
        anchor: u32,
        name: &str,
    ) -> SimpleExtensionDeclaration {
        SimpleExtensionDeclaration {
            mapping_type: Some(MappingType::ExtensionFunction(
                proto::extensions::simple_extension_declaration::ExtensionFunction {
                    extension_uri_reference: uri_ref,
                    extension_urn_reference: uri_ref,
                    function_anchor: anchor,
                    name: name.to_string(),
                },
            )),
        }
    }

    fn datatype_to_substrait(dt: &DataType) -> proto::Type {
        let kind = match dt {
            DataType::Int8 => Kind::I8(proto::r#type::I8 {
                type_variation_reference: 0,
                nullability: Nullability::Nullable as i32,
            }),
            DataType::Int16 => Kind::I16(proto::r#type::I16 {
                type_variation_reference: 0,
                nullability: Nullability::Nullable as i32,
            }),
            DataType::Int32 => Kind::I32(proto::r#type::I32 {
                type_variation_reference: 0,
                nullability: Nullability::Nullable as i32,
            }),
            DataType::Int64 => Kind::I64(proto::r#type::I64 {
                type_variation_reference: 0,
                nullability: Nullability::Nullable as i32,
            }),
            DataType::UInt8 | DataType::UInt16 | DataType::UInt32 | DataType::UInt64 => {
                Kind::I64(proto::r#type::I64 {
                    type_variation_reference: 0,
                    nullability: Nullability::Nullable as i32,
                })
            }
            DataType::Float32 => Kind::Fp32(proto::r#type::Fp32 {
                type_variation_reference: 0,
                nullability: Nullability::Nullable as i32,
            }),
            DataType::Float64 => Kind::Fp64(proto::r#type::Fp64 {
                type_variation_reference: 0,
                nullability: Nullability::Nullable as i32,
            }),
            DataType::Boolean => Kind::Bool(proto::r#type::Boolean {
                type_variation_reference: 0,
                nullability: Nullability::Nullable as i32,
            }),
            DataType::Utf8 | DataType::LargeUtf8 => Kind::String(proto::r#type::String {
                type_variation_reference: 0,
                nullability: Nullability::Nullable as i32,
            }),
            DataType::Date32 | DataType::Date64 => Kind::Date(proto::r#type::Date {
                type_variation_reference: 0,
                nullability: Nullability::Nullable as i32,
            }),
            DataType::TimestampSecond
            | DataType::TimestampMillisecond
            | DataType::TimestampMicrosecond => {
                let precision = match dt {
                    DataType::TimestampSecond => 0,
                    DataType::TimestampMillisecond => 3,
                    _ => 6,
                };
                Kind::PrecisionTimestamp(proto::r#type::PrecisionTimestamp {
                    precision,
                    type_variation_reference: 0,
                    nullability: Nullability::Nullable as i32,
                })
            }
            DataType::Duration => Kind::IntervalDay(proto::r#type::IntervalDay {
                precision: None,
                type_variation_reference: 0,
                nullability: Nullability::Nullable as i32,
            }),
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
            DataType::List(_) | DataType::Struct(_) => {
                Kind::String(proto::r#type::String {
                    type_variation_reference: 0,
                    nullability: Nullability::Nullable as i32,
                })
            }
        };

        proto::Type { kind: Some(kind) }
    }

    #[allow(deprecated)]
    fn substrait_to_datatype(typ: &proto::Type) -> DataType {
        match &typ.kind {
            Some(Kind::I8(_)) => DataType::Int8,
            Some(Kind::I16(_)) => DataType::Int16,
            Some(Kind::I32(_)) => DataType::Int32,
            Some(Kind::I64(_)) => DataType::Int64,
            Some(Kind::Fp32(_)) => DataType::Float32,
            Some(Kind::Fp64(_)) => DataType::Float64,
            Some(Kind::Bool(_)) => DataType::Boolean,
            Some(Kind::String(_)) => DataType::Utf8,
            Some(Kind::Date(_)) => DataType::Date32,
            Some(Kind::PrecisionTimestamp(_)) | Some(Kind::Timestamp(_)) => {
                DataType::TimestampMicrosecond
            }
            Some(Kind::Decimal(d)) => DataType::Decimal {
                precision: d.precision as u8,
                scale: d.scale as i8,
            },
            Some(Kind::Binary(_)) => DataType::Binary,
            _ => DataType::Utf8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_roundtrip() {
        let schema = Schema::new(vec![
            Field::new("id", DataType::Int64),
            Field::new("amount", DataType::Float64),
        ]);

        let scan = ScanNode {
            meta: NodeMeta::new(schema),
            table_name: "orders".to_string(),
            location: None,
            format: None,
            projection: vec!["id".to_string(), "amount".to_string()],
        };

        let plan = LogicalPlan::new(
            PlanNode::Scan(scan),
            vec!["id".to_string(), "amount".to_string()],
        );

        let substrait = SubstraitSerializer::to_substrait(&plan).unwrap();
        let back = SubstraitSerializer::from_substrait(&substrait).unwrap();

        assert_eq!(plan.output_names, back.output_names);
    }

    #[test]
    fn test_filter_roundtrip() {
        let schema = Schema::new(vec![Field::new("amount", DataType::Float64)]);

        let scan = ScanNode {
            meta: NodeMeta::new(schema.clone()),
            table_name: "orders".to_string(),
            location: None,
            format: None,
            projection: vec!["amount".to_string()],
        };

        let filter = FilterNode {
            meta: NodeMeta::new(schema),
            input: Box::new(PlanNode::Scan(scan)),
            predicate: Expr::gt(Expr::column("amount"), Expr::float(100.0)),
        };

        let plan = LogicalPlan::new(
            PlanNode::Filter(filter),
            vec!["amount".to_string()],
        );

        let substrait = SubstraitSerializer::to_substrait(&plan).unwrap();
        let back = SubstraitSerializer::from_substrait(&substrait).unwrap();

        assert_eq!(plan.output_names, back.output_names);
    }

    fn make_scan(table: &str) -> PlanNode {
        let schema = Schema::new(vec![
            Field::new("id", DataType::Int64),
            Field::new("amount", DataType::Float64),
        ]);
        PlanNode::Scan(ScanNode {
            meta: NodeMeta::new(schema),
            table_name: table.to_string(),
            location: None,
            format: None,
            projection: vec!["id".to_string(), "amount".to_string()],
        })
    }

    #[test]
    fn test_union_all_roundtrip() {
        let schema = Schema::new(vec![
            Field::new("id", DataType::Int64),
            Field::new("amount", DataType::Float64),
        ]);
        let union_node = UnionNode {
            meta: NodeMeta::new(schema),
            inputs: vec![make_scan("orders_a"), make_scan("orders_b")],
            distinct: false,
        };

        let plan = LogicalPlan::new(
            PlanNode::Union(union_node),
            vec!["id".to_string(), "amount".to_string()],
        );

        let substrait = SubstraitSerializer::to_substrait(&plan).unwrap();
        let back = SubstraitSerializer::from_substrait(&substrait).unwrap();

        match &back.root {
            PlanNode::Union(u) => assert!(!u.distinct, "expected UNION ALL (distinct=false)"),
            other => panic!("expected Union node, got {:?}", other),
        }
    }

    #[test]
    fn test_union_distinct_roundtrip() {
        let schema = Schema::new(vec![
            Field::new("id", DataType::Int64),
            Field::new("amount", DataType::Float64),
        ]);
        let union_node = UnionNode {
            meta: NodeMeta::new(schema),
            inputs: vec![make_scan("orders_a"), make_scan("orders_b")],
            distinct: true,
        };

        let plan = LogicalPlan::new(
            PlanNode::Union(union_node),
            vec!["id".to_string(), "amount".to_string()],
        );

        let substrait = SubstraitSerializer::to_substrait(&plan).unwrap();
        let back = SubstraitSerializer::from_substrait(&substrait).unwrap();

        match &back.root {
            PlanNode::Union(u) => assert!(u.distinct, "expected UNION DISTINCT (distinct=true)"),
            other => panic!("expected Union node, got {:?}", other),
        }
    }
}
