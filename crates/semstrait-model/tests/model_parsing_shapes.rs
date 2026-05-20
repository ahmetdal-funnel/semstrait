//! End-to-end parse + build smoke test over a YAML model fixture.
//!
//! Exercises every author-facing shape produced by the parser:
//! the closed sugar roster (`concat:` / `upper:` / `eq:` / `in:` /
//! `regexp_match:` / `regexp_extract:` / `case:`); the unified `lit:`
//! literal keyword across both expression bodies and
//! `semantic_mapping:`; bare-scalar = column rule for mapping values;
//! `Inline(_)` arm for unresolved DSL strings; the flattened
//! `extras.temporal:` shape; `keys:` with `KeyDecl` wrapper.
//!
//! Paired with the deep-equality test in `model_parsing.rs`. This file
//! is the fast shape-tag sanity check; the deep test asserts exact IR.

use semstrait_ir::{AggregationOp, BinaryOpKind, CanonicalFn, Expr, Literal, SemanticLeaf};
use semstrait_model::{parse, DimensionEntry, ExprSource, SemanticMappingValue};

const FIXTURE: &str = include_str!("../../../test_data/alpinestars_eu_ad_platform_v3.yaml");

/// Walk an `Expr<SemanticLeaf>` collecting one `&'static str` tag per
/// node in pre-order — `BinaryOp(eq)`, `Aggregate(Sum)`, `Field`, etc.
/// Used by the structural assertions below to verify the parser folded
/// sugar tags into the IR shapes we ratified, not just "anything".
fn shape_tags(expr: &Expr<SemanticLeaf>) -> Vec<String> {
    use semstrait_ir::tree::Visitor;
    use std::ops::ControlFlow;
    struct Tags(Vec<String>);
    impl Visitor<Expr<SemanticLeaf>> for Tags {
        type Output = ();
        fn f_down(&mut self, n: &Expr<SemanticLeaf>) -> ControlFlow<()> {
            self.0.push(match n {
                Expr::Leaf(SemanticLeaf::Literal(_)) => "Literal".into(),
                Expr::Leaf(SemanticLeaf::Field(_)) => "Field".into(),
                Expr::Leaf(SemanticLeaf::Column(_)) => "Column".into(),
                Expr::Leaf(SemanticLeaf::Dimension { .. }) => "Dimension".into(),
                Expr::Leaf(SemanticLeaf::Measure { .. }) => "Measure".into(),
                Expr::Leaf(SemanticLeaf::Metric { .. }) => "Metric".into(),
                Expr::Leaf(SemanticLeaf::Key { .. }) => "Key".into(),
                Expr::Leaf(_) => "Leaf?".into(),
                Expr::BinaryOp { op, .. } => format!("BinaryOp({op:?})"),
                Expr::UnaryOp { op, .. } => format!("UnaryOp({op:?})"),
                Expr::FunctionCall { name, .. } => format!("FunctionCall({})", name.0),
                Expr::Cast { .. } => "Cast".into(),
                Expr::Case { .. } => "Case".into(),
                Expr::InList { negated, .. } => format!("InList(neg={negated})"),
                Expr::Between { .. } => "Between".into(),
                Expr::Like { kind, .. } => format!("Like({kind:?})"),
                Expr::IsNull(_) => "IsNull".into(),
                Expr::Coalesce(_) => "Coalesce".into(),
                Expr::NullIf { .. } => "NullIf".into(),
                Expr::Aggregate { op, distinct, .. } => format!("Aggregate({op:?},dist={distinct})"),
                Expr::Window { .. } => "Window".into(),
                _ => "?".into(),
            });
            ControlFlow::Continue(())
        }
        fn f_up(&mut self, _n: &Expr<SemanticLeaf>) -> ControlFlow<()> {
            ControlFlow::Continue(())
        }
    }
    let mut tags = Tags(Vec::new());
    use semstrait_ir::tree::Tree;
    let _ = expr.apply(&mut tags);
    tags.0
}

#[test]
fn model_parsing_produces_expected_shapes() {
    let builder = match parse(FIXTURE) {
        Ok(b) => b,
        Err(diags) => {
            for d in &diags {
                eprintln!("PARSE: {:?}", d);
            }
            panic!("model fixture failed to parse");
        }
    };
    let (model, build_diags) = match builder.build() {
        Ok(pair) => pair,
        Err(diags) => {
            for d in &diags {
                eprintln!("BUILD: {:?}", d);
            }
            panic!("model fixture failed to build");
        }
    };
    for d in &build_diags {
        eprintln!("BUILD-WARN: {:?}", d);
    }

    // Top-level shape ---------------------------------------------------
    assert_eq!(model.name, "alpinestars-eu-ad-platforms");
    assert!(model.unionsets.contains_key("paid_media_campaign_performance"));
    assert!(model.datasets.contains_key("shopify"));
    // Shared pools — guards against silent fixture drift.
    // (`market` is computed-only at each Public site; not in the pool.)
    assert_eq!(model.dimensions.len(), 9);
    assert_eq!(model.measures.len(), 6);
    assert_eq!(model.metrics.len(), 6);

    // measurement_source — `concat:` sugar lowers to FunctionCall(CONCAT)
    // with three children: bare-name `Field`, lit String, bare-name `Field`.
    let measurement_source = model
        .dimensions
        .get("measurement_source")
        .expect("measurement_source dimension present");
    let expr = measurement_source.expr.as_ref().expect("has expr");
    let inner = match expr {
        ExprSource::Block(e) => e,
        other => panic!("expected Block expr, got {other:?}"),
    };
    match inner {
        Expr::FunctionCall { name, args } => {
            assert_eq!(name, &CanonicalFn("CONCAT".into()), "concat: → CONCAT");
            assert_eq!(args.len(), 3, "concat takes 3 args");
            // First and third args are bare-name `Field`s (token rule).
            assert!(matches!(&args[0], Expr::Leaf(SemanticLeaf::Field(n)) if n.0 == "measurement_channel"));
            assert!(matches!(
                &args[1],
                Expr::Leaf(SemanticLeaf::Literal(Literal::String(s))) if s == " - "
            ));
            assert!(matches!(&args[2], Expr::Leaf(SemanticLeaf::Field(n)) if n.0 == "traffic_source"));
        }
        other => panic!("expected FunctionCall(CONCAT), got {other:?}"),
    }

    // Inline-DSL metric expressions still surface as Inline (deferred).
    let cpc = model.metrics.get("cpc").expect("cpc metric present");
    let cpc_expr = cpc.expr.as_ref().expect("cpc has expr");
    assert!(
        matches!(cpc_expr, ExprSource::Inline(s) if s.contains("cost / clicks")),
        "expected Inline expr for cpc, got {cpc_expr:?}"
    );

    // Unionset's `market` computed dim — case+eq+in+upper+regexp_match
    // sugar nest. Assert the IR shape contains the expected sugar
    // landings (Case roots, BinaryOp(Eq) branches, InList, FunctionCall).
    let unionset = model
        .unionsets
        .get("paid_media_campaign_performance")
        .expect("unionset present");
    let market_entry = unionset
        .semantic_interface
        .dimensions
        .iter()
        .find(|d: &&DimensionEntry| d.name().as_str() == "market")
        .expect("market dim attached to unionset");
    let market_expr = match market_entry {
        DimensionEntry::Inline(d) => match d.expr.as_ref().expect("has expr") {
            ExprSource::Block(e) => e,
            other => panic!("market: expected Block, got {other:?}"),
        },
        other => panic!("computed `market` should be inline, got {other:?}"),
    };
    assert!(matches!(market_expr, Expr::Case { .. }), "market root = Case");
    let tags = shape_tags(market_expr);
    // We expect the full sugar roster to land:
    assert!(tags.iter().any(|t| t.starts_with("Case")), "Case present");
    assert!(
        tags.iter().any(|t| t == "BinaryOp(Eq)"),
        "eq: sugar → BinaryOp(Eq); tags={tags:?}"
    );
    assert!(
        tags.iter().any(|t| t == "InList(neg=false)"),
        "in: sugar → InList(negated=false); tags={tags:?}"
    );
    assert!(
        tags.iter().any(|t| t == "FunctionCall(UPPER)"),
        "upper: sugar → FunctionCall(UPPER); tags={tags:?}"
    );
    assert!(
        tags.iter().any(|t| t == "FunctionCall(REGEXP_MATCH)"),
        "regexp_match: sugar → FunctionCall(REGEXP_MATCH); tags={tags:?}"
    );
    assert!(
        tags.iter().any(|t| t == "FunctionCall(REGEXP_EXTRACT)"),
        "regexp_extract: sugar → FunctionCall(REGEXP_EXTRACT); tags={tags:?}"
    );

    // Shopify dataset: `market` cascade uses `eq: [country, {lit: "..."}]`
    // shorthand. Assert it landed as BinaryOp(Eq) too — confirms the
    // sequence-form sugar parses identically to the map-form.
    let shopify = model.datasets.get("shopify").expect("shopify present");
    let shopify_market = shopify
        .semantic_interface
        .dimensions
        .iter()
        .find(|d: &&DimensionEntry| d.name().as_str() == "market")
        .expect("shopify.market attached");
    let shopify_market_expr = match shopify_market {
        DimensionEntry::Inline(d) => match d.expr.as_ref().expect("has expr") {
            ExprSource::Block(e) => e,
            other => panic!("shopify.market: expected Block, got {other:?}"),
        },
        other => panic!("shopify.market should be inline, got {other:?}"),
    };
    let stags = shape_tags(shopify_market_expr);
    assert!(
        stags.iter().filter(|t| t.as_str() == "BinaryOp(Eq)").count() == 5,
        "shopify.market has 5 country branches; tags={stags:?}"
    );

    // semantic_mapping: bare scalar = Column, `lit:` = Literal.
    // Pick the adwords dataset's mapping and verify both shapes landed.
    let adwords = unionset
        .body
        .datasets
        .iter()
        .find(|d| d.body.base.name == "adwords_campaign_data")
        .expect("adwords nested dataset present");
    let mapping = match &adwords.body.base.extras.semantic_mapping {
        semstrait_model::SemanticMapping::Explicit(m) => m,
        other => panic!("expected Explicit mapping, got {other:?}"),
    };
    // bare scalar `date: date` → Column("date")
    let date_v = mapping
        .get(&semstrait_model::SemanticsName("date".into()))
        .expect("date mapping present");
    assert!(matches!(date_v, SemanticMappingValue::Column(s) if s == "date"));
    // `measurement_channel: { lit: { string: "Paid Search" } }` → Literal
    let mc_v = mapping
        .get(&semstrait_model::SemanticsName("measurement_channel".into()))
        .expect("measurement_channel mapping present");
    assert!(
        matches!(
            mc_v,
            SemanticMappingValue::Literal(semstrait_model::LiteralValue::String(s))
                if s == "Paid Search"
        ),
        "expected Literal(String('Paid Search')), got {mc_v:?}"
    );
    // `actions: { lit: { int: 0 } }` → Literal::Int(0)
    let actions_v = mapping
        .get(&semstrait_model::SemanticsName("actions".into()))
        .expect("actions mapping present");
    assert!(
        matches!(
            actions_v,
            SemanticMappingValue::Literal(semstrait_model::LiteralValue::Int(0))
        ),
        "expected Literal(Int(0)), got {actions_v:?}"
    );

    // Sanity touch on aggregate-related model state.
    let cost = model.measures.get("cost").expect("cost measure present");
    assert_eq!(cost.agg, semstrait_model::AggregationType::Sum);
    // suppress unused-import lints for re-exports we exercise via traits.
    let _ = (
        BinaryOpKind::Eq,
        AggregationOp::Sum,
        Literal::Null,
        CanonicalFn("X".into()),
    );

    // Datasets attached.
    assert_eq!(unionset.body.datasets.len(), 6);
}
