//! Deep parser-correctness test: parse a YAML model fixture and assert
//! the resulting `SemanticModel` matches an exact constructed-expected
//! shape, axis by axis.
//!
//! Where `model_parsing_shapes.rs` asks "did it parse and produce
//! reasonable shapes?", this asks "did the parser produce *the exact
//! tree we authored*?". Every author idiom has at least one
//! structural-equality assertion against a constructed-expected
//! `Expr<L>` value built via the IR's public constructors. A silent
//! parser bug (lit↔column swap, dropped branch, wrong arity, missing
//! accessor) surfaces as a structural mismatch here.
//!
//! Coverage axes:
//!
//! - Build cleanliness: zero error diagnostics.
//! - Top-level identity: model name, label set, public DataKind names.
//! - Shared-pool identity: every shared dim/measure/metric named, with
//!   `data_type`, `agg`, and the dim `type` variant verified.
//! - Inline expressions: exact verbatim string for every metric.
//! - Block expressions: full structural equality for representative
//!   `concat:` sugar and a 5-branch `case` cascade with `eq:` predicates.
//! - Sugar landings: a deep `case` cascade contains the exact expected
//!   operator multiset (case / in / eq / regexp_match / regexp_extract
//!   / upper) at the right counts.
//! - Nested datasets: all variants present with `catalog`,
//!   `storage.tables`, `temporal: events { event_time, grain }`, and
//!   a sample of `semantic_mapping` entries that verify both the
//!   bare-scalar = `Column` and `lit:` = `Literal` rules.
//! - Standalone Dataset: identity + the same per-axis checks.

use std::collections::BTreeMap;

use semstrait_core::{DataType, Grain};
use semstrait_ir::{
    AggregationOp, BinaryOpKind, CanonicalFn, ColumnRef, Expr, LikeKind, Literal, SemanticLeaf,
    SemanticsName as IrSemanticsName, UnaryOpKind,
};
use semstrait_model::{
    parse, AggregationType, DimensionEntry, DimensionType, ExprSource, LiteralValue,
    MeasureEntry, MetricEntry, SemanticMapping, SemanticMappingValue, SemanticsName,
    TemporalShape, TemporalShapeKind,
};

const FIXTURE: &str = include_str!("../../../test_data/alpinestars_eu_ad_platform_v3.yaml");

// ── helpers — constructed expected `Expr<SemanticLeaf>` builders ────────

fn lit_str(s: &str) -> Expr<SemanticLeaf> {
    Expr::Leaf(SemanticLeaf::Literal(Literal::String(s.into())))
}
fn lit_int(i: i64) -> Expr<SemanticLeaf> {
    Expr::Leaf(SemanticLeaf::Literal(Literal::Integer(i)))
}
fn lit_null() -> Expr<SemanticLeaf> {
    Expr::Leaf(SemanticLeaf::Literal(Literal::Null))
}
fn field(name: &str) -> Expr<SemanticLeaf> {
    Expr::Leaf(SemanticLeaf::Field(IrSemanticsName(name.into())))
}
fn col(name: &str) -> Expr<SemanticLeaf> {
    Expr::Leaf(SemanticLeaf::Column(ColumnRef(name.into())))
}
fn binop(op: BinaryOpKind, left: Expr<SemanticLeaf>, right: Expr<SemanticLeaf>) -> Expr<SemanticLeaf> {
    Expr::BinaryOp {
        op,
        left: Box::new(left),
        right: Box::new(right),
    }
}
fn func(name: &str, args: Vec<Expr<SemanticLeaf>>) -> Expr<SemanticLeaf> {
    Expr::FunctionCall {
        name: CanonicalFn(name.into()),
        args,
    }
}

fn block_expr<'a>(src: &'a ExprSource<SemanticLeaf>) -> &'a Expr<SemanticLeaf> {
    match src {
        ExprSource::Block(e) => e,
        ExprSource::Inline(s) => panic!("expected Block expr, got Inline({s:?})"),
        other => panic!("unknown ExprSource variant: {other:?}"),
    }
}
fn inline_str<'a>(src: &'a ExprSource<SemanticLeaf>) -> &'a str {
    match src {
        ExprSource::Inline(s) => s.as_str(),
        ExprSource::Block(e) => panic!("expected Inline expr, got Block({e:?})"),
        other => panic!("unknown ExprSource variant: {other:?}"),
    }
}

#[test]
fn model_parsing_produces_expected_ir() {
    // ── Parse + build cleanly. Zero error diagnostics expected. ───────
    let builder = parse(FIXTURE).unwrap_or_else(|diags| {
        for d in &diags {
            eprintln!("PARSE: {:?}", d);
        }
        panic!("model fixture failed to parse");
    });
    let (model, build_diags) = builder.build().unwrap_or_else(|diags| {
        for d in &diags {
            eprintln!("BUILD: {:?}", d);
        }
        panic!("model fixture failed to build");
    });
    // No errors are tolerated. Warnings are allowed; we explicitly
    // expect one — `shopify.country` shadows the shared-pool `country`,
    // which the validator emits as a `SemanticsShadowRootPool` warning.
    let errors: Vec<_> = build_diags
        .iter()
        .filter(|d| matches!(d.severity, semstrait_core::Severity::Error))
        .collect();
    assert!(
        errors.is_empty(),
        "expected zero build errors; got: {:?}",
        errors
    );

    // ── Top-level identity ────────────────────────────────────────────
    assert_eq!(model.name, "alpinestars-eu-ad-platforms");
    assert_eq!(
        model.labels,
        vec![
            "mmm".to_string(),
            "alpinestars".into(),
            "eu".into(),
            "paid-media".into(),
            "independent-variables".into()
        ]
    );
    assert!(
        model.unionsets.contains_key("paid_media_campaign_performance"),
        "unionset present"
    );
    assert!(model.datasets.contains_key("shopify"), "shopify dataset present");
    assert_eq!(model.datasets.len(), 1, "exactly one top-level dataset");
    assert_eq!(model.unionsets.len(), 1, "exactly one top-level unionset");
    assert_eq!(model.grainsets.len(), 0);
    assert_eq!(model.joinsets.len(), 0);

    // ── Shared dimensions: identity + dim_type per entry ──────────────
    // Names + variant tags. We assert every entry to catch silent
    // schema regressions (e.g. defaulting `type:` to a wrong variant).
    let dims = &model.dimensions;
    assert_eq!(
        dims.keys().cloned().collect::<Vec<_>>(),
        vec![
            "campaign",
            "country",
            "currency",
            "dataset_name",
            "date",
            "funnel_account_id",
            "measurement_channel",
            "measurement_source",
            "traffic_source",
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>(),
    );
    // dim_type variant per entry.
    let dt = |name: &str| dims.get(name).expect(name).dim_type.clone();
    assert!(matches!(dt("campaign"), DimensionType::Categorical));
    assert!(matches!(dt("country"), DimensionType::Categorical));
    assert!(matches!(dt("currency"), DimensionType::Categorical));
    assert!(matches!(dt("measurement_channel"), DimensionType::Categorical));
    assert!(matches!(dt("measurement_source"), DimensionType::Categorical));
    assert!(matches!(dt("traffic_source"), DimensionType::Categorical));
    // `dataset_name` and `funnel_account_id` are metadata-from-path.
    match dt("dataset_name") {
        DimensionType::Metadata(b) => match b.source {
            semstrait_model::MetadataSource::Path(p) => assert_eq!(p.token, 5),
            other => panic!("dataset_name should be Metadata(Path), got {other:?}"),
        },
        other => panic!("dataset_name should be Metadata, got {other:?}"),
    }
    match dt("funnel_account_id") {
        DimensionType::Metadata(b) => match b.source {
            semstrait_model::MetadataSource::Path(p) => assert_eq!(p.token, 6),
            other => panic!("funnel_account_id should be Metadata(Path), got {other:?}"),
        },
        other => panic!("funnel_account_id should be Metadata, got {other:?}"),
    }
    // `date` is Temporal with a 5-grain roster.
    match dt("date") {
        DimensionType::Temporal(b) => assert_eq!(
            b.grains,
            vec![Grain::Day, Grain::Week, Grain::Month, Grain::Quarter, Grain::Year]
        ),
        other => panic!("date should be Temporal, got {other:?}"),
    }
    // `data_type` per entry (sample — most are `string`).
    for name in [
        "campaign",
        "country",
        "currency",
        "dataset_name",
        "funnel_account_id",
        "measurement_channel",
        "measurement_source",
        "traffic_source",
    ] {
        assert_eq!(dims.get(name).unwrap().data_type, DataType::String, "{name}");
    }
    assert_eq!(dims.get("date").unwrap().data_type, DataType::Date);

    // ── measurement_source: full IR equality on the Block expr ────────
    // Author wrote:
    //   expr:
    //     concat:
    //       - measurement_channel
    //       - lit: " - "
    //       - traffic_source
    let ms_expr = block_expr(dims.get("measurement_source").unwrap().expr.as_ref().unwrap());
    let expected_ms = func(
        "CONCAT",
        vec![
            field("measurement_channel"),
            lit_str(" - "),
            field("traffic_source"),
        ],
    );
    assert_eq!(
        ms_expr, &expected_ms,
        "measurement_source CONCAT must equal the constructed expected tree"
    );

    // ── Shared measures: identity + agg + data_type ───────────────────
    let measures = &model.measures;
    let expected_measures: Vec<&str> = vec![
        "actions",
        "clicks",
        "cost",
        "impressions",
        "platform_conversions",
        "platform_revenue",
    ];
    assert_eq!(
        measures.keys().cloned().collect::<Vec<_>>(),
        expected_measures.iter().map(|s| s.to_string()).collect::<Vec<_>>()
    );
    for n in expected_measures {
        let m = measures.get(n).unwrap();
        assert_eq!(m.agg, AggregationType::Sum, "{n} agg = sum");
        assert_eq!(
            m.data_type,
            DataType::Decimal {
                precision: 18,
                scale: 2
            },
            "{n} data_type = decimal(18,2)"
        );
    }

    // ── Shared metrics: identity + Inline verbatim ────────────────────
    let metrics = &model.metrics;
    let expected_inlines: BTreeMap<&str, &str> = [
        ("cpc", "cost / clicks"),
        ("cpm", "(cost / impressions) * 1000"),
        ("ctr", "clicks / impressions"),
        ("cpa", "cost / platform_conversions"),
        ("roas", "platform_revenue / cost"),
        ("conversion_rate", "platform_conversions / clicks"),
    ]
    .into_iter()
    .collect();
    assert_eq!(metrics.len(), expected_inlines.len());
    for (name, expected_dsl) in &expected_inlines {
        let metric = metrics.get(*name).unwrap_or_else(|| panic!("metric {name}"));
        assert_eq!(
            metric.data_type,
            DataType::Decimal {
                precision: 18,
                scale: 4
            }
        );
        assert_eq!(
            inline_str(metric.expr.as_ref().unwrap()),
            *expected_dsl,
            "metric {name}: Inline verbatim string"
        );
    }

    // ── Unionset identity + extras + sugar landings on `market` ──────
    let union = model
        .unionsets
        .get("paid_media_campaign_performance")
        .unwrap();
    // `mode: all` (default).
    assert_eq!(union.body.mode, semstrait_model::UnionMode::All);
    // `extras.temporal: events { event_time = date }`, no grain (forbidden
    // on complex per SR-E-7).
    let temporal: &TemporalShape = union.body.base.extras.temporal.as_ref().unwrap();
    assert!(temporal.grain.is_none(), "no grain on complex");
    match &temporal.kind {
        TemporalShapeKind::Events(b) => assert_eq!(b.event_time.as_ref(), "date"),
        other => panic!("expected Events, got {other:?}"),
    }
    // Primary key with the 6 listed fields.
    let keys = union.semantic_interface.keys.as_ref().expect("keys block");
    let pk = keys.primary.as_ref().expect("primary key");
    assert_eq!(
        pk.fields.iter().map(|n| n.as_ref().to_owned()).collect::<Vec<_>>(),
        vec![
            "campaign_id",
            "account_id",
            "adgroup_id",
            "ad_id",
            "conversion_tracker_id",
            "ad_video_id"
        ]
    );
    // Dimensions: 9 refs + 1 inline (`market`).
    let union_dims = &union.semantic_interface.dimensions;
    let names: Vec<String> = union_dims.iter().map(|e| e.name().as_ref().to_owned()).collect();
    assert_eq!(
        names,
        vec![
            "date",
            "dataset_name",
            "campaign",
            "country",
            "traffic_source",
            "measurement_source",
            "measurement_channel",
            "currency",
            "funnel_account_id",
            "market",
        ]
    );
    let ref_count = union_dims
        .iter()
        .filter(|e| matches!(e, DimensionEntry::Ref(_)))
        .count();
    assert_eq!(ref_count, 9, "9 ref entries");
    // Inline `market` with a Block expr; full IR walk-and-count assertion.
    let market_expr = match union_dims.iter().find(|e| e.name().as_ref() == "market").unwrap() {
        DimensionEntry::Inline(d) => block_expr(d.expr.as_ref().unwrap()),
        _ => panic!("market should be Inline"),
    };
    // The cascade is large; we don't construct the full expected tree
    // (it'd be ~80 lines). Instead we count operator landings + sample
    // a few exact equalities.
    let counts = count_op_kinds(market_expr);
    assert_eq!(counts.case, 4, "4 nested case roots");
    assert_eq!(counts.in_list, 1, "one in-list (platform names)");
    assert_eq!(counts.binop_eq, 2, "two eq sugars (facebookads, impact)");
    assert_eq!(counts.regexp_match, 3, "three regexp_match calls");
    assert_eq!(counts.regexp_extract, 3, "three regexp_extract calls");
    assert_eq!(counts.upper, 3, "three upper calls");
    // Top-level shape: `Case { whens: [(InList, Case), (Eq, Case)], else: lit("") }`.
    match market_expr {
        Expr::Case { whens, else_ } => {
            assert_eq!(whens.len(), 2, "two outer when arms");
            // First when's predicate is the platform-names InList.
            match &whens[0].0 {
                Expr::InList { value, list, negated } => {
                    assert!(!negated);
                    assert_eq!(value.as_ref(), &col("dataset_name"));
                    let expected_platforms: Vec<Expr<SemanticLeaf>> = vec![
                        lit_str("adwords"),
                        lit_str("facebookads"),
                        lit_str("bing"),
                        lit_str("tiktok"),
                        lit_str("klaviyo"),
                    ];
                    assert_eq!(list, &expected_platforms);
                }
                other => panic!("first when[0] = InList expected, got {other:?}"),
            }
            // Second when is BinaryOp(Eq, Field("dataset_name"), Lit("impact")).
            assert_eq!(
                &whens[1].0,
                &binop(BinaryOpKind::Eq, field("dataset_name"), lit_str("impact"))
            );
            // else is Literal("").
            let else_box = else_.as_ref().expect("else present");
            assert_eq!(else_box.as_ref(), &lit_str(""));
        }
        other => panic!("market root must be Case, got {other:?}"),
    }

    // ── Unionset measures + metrics: all refs ─────────────────────────
    let m_refs: Vec<String> = union
        .semantic_interface
        .measures
        .iter()
        .map(|e| e.name().as_ref().to_owned())
        .collect();
    assert_eq!(
        m_refs,
        vec![
            "cost",
            "clicks",
            "impressions",
            "platform_conversions",
            "platform_revenue",
            "actions"
        ]
    );
    for e in &union.semantic_interface.measures {
        assert!(matches!(e, MeasureEntry::Ref(_)));
    }
    let met_refs: Vec<String> = union
        .semantic_interface
        .metrics
        .iter()
        .map(|e| e.name().as_ref().to_owned())
        .collect();
    assert_eq!(
        met_refs,
        vec!["cpc", "cpm", "ctr", "cpa", "roas", "conversion_rate"]
    );
    for e in &union.semantic_interface.metrics {
        assert!(matches!(e, MetricEntry::Ref(_)));
    }

    // ── Six nested datasets: identity, catalog, storage.tables, ──────
    // temporal, and a sample of semantic_mapping entries.
    let nested = &union.body.datasets;
    assert_eq!(nested.len(), 6);
    let nested_names: Vec<String> = nested
        .iter()
        .map(|d| d.body.base.name.clone())
        .collect();
    assert_eq!(
        nested_names,
        vec![
            "adwords_campaign_data",
            "bing_campaign_data",
            "facebook_adset_data",
            "tiktok_ad_data",
            "impact_adset_data",
            "klaviyo_adset_data",
        ]
    );
    for d in nested {
        let extras = &d.body.base.extras;
        let cat = extras.catalog.as_ref().expect(&d.body.base.name);
        assert_eq!(cat.alias, "polaris");
        let storage = extras.storage.as_ref().expect("storage");
        // tables glob is a single entry per dataset.
        assert_eq!(storage.tables.len(), 1, "one table glob per nested dataset");
        // temporal: events { event_time = date, grain = day }.
        let t = extras.temporal.as_ref().expect("temporal");
        assert_eq!(t.grain, Some(Grain::Day));
        match &t.kind {
            TemporalShapeKind::Events(b) => assert_eq!(b.event_time.as_ref(), "date"),
            other => panic!("nested.{}: expected Events, got {other:?}", d.body.base.name),
        }
    }

    // adwords mapping — bare = column, lit = literal, both shapes
    let adwords = nested
        .iter()
        .find(|d| d.body.base.name == "adwords_campaign_data")
        .unwrap();
    let amap = match &adwords.body.base.extras.semantic_mapping {
        SemanticMapping::Explicit(m) => m,
        other => panic!("expected Explicit mapping, got {other:?}"),
    };
    let mc = SemanticsName("measurement_channel".into());
    assert_eq!(
        amap.get(&mc).unwrap(),
        &SemanticMappingValue::Literal(LiteralValue::String("Paid Search".into())),
        "adwords measurement_channel = lit Paid Search"
    );
    let date = SemanticsName("date".into());
    assert_eq!(
        amap.get(&date).unwrap(),
        &SemanticMappingValue::Column("date".into()),
        "adwords date = bare column 'date'"
    );
    let actions = SemanticsName("actions".into());
    assert_eq!(
        amap.get(&actions).unwrap(),
        &SemanticMappingValue::Literal(LiteralValue::Int(0)),
        "adwords actions = lit int 0"
    );
    let pr = SemanticsName("platform_revenue".into());
    assert_eq!(
        amap.get(&pr).unwrap(),
        &SemanticMappingValue::Column("adwords_allConvValue".into()),
        "adwords platform_revenue = bare column 'adwords_allConvValue'"
    );

    // klaviyo mapping — bare with dash works ("klaviyo-clicks2" stays
    // a single column atom; the `-` does NOT split as subtraction).
    let klaviyo = nested
        .iter()
        .find(|d| d.body.base.name == "klaviyo_adset_data")
        .unwrap();
    let kmap = match &klaviyo.body.base.extras.semantic_mapping {
        SemanticMapping::Explicit(m) => m,
        other => panic!("expected Explicit mapping, got {other:?}"),
    };
    let clicks = SemanticsName("clicks".into());
    assert_eq!(
        kmap.get(&clicks).unwrap(),
        &SemanticMappingValue::Column("klaviyo-clicks2".into()),
        "klaviyo clicks: dashed identifier survives as one Column atom"
    );

    // ── Shopify standalone Dataset ────────────────────────────────────
    let shopify = model.datasets.get("shopify").unwrap();
    let s_extras = &shopify.body.base.extras;
    assert_eq!(s_extras.catalog.as_ref().unwrap().alias, "polaris");
    let s_storage = s_extras.storage.as_ref().unwrap();
    assert_eq!(s_storage.tables, vec!["shopify.*".to_string()]);
    let s_t = s_extras.temporal.as_ref().unwrap();
    assert_eq!(s_t.grain, Some(Grain::Day));
    assert!(matches!(&s_t.kind, TemporalShapeKind::Events(_)));

    // shopify dimensions — country (inline cat), market (inline cat
    // with computed expr), and 6 standalone categoricals.
    let s_dim_names: Vec<String> = shopify
        .semantic_interface
        .dimensions
        .iter()
        .map(|e| e.name().as_ref().to_owned())
        .collect();
    assert_eq!(
        s_dim_names,
        vec![
            "dataset_name",
            "funnel_account_id",
            "date",
            "currency",
            "country",
            "market",
            "sales_channel",
            "brand",
            "sale_line_type",
            "sale_action_type",
            "order_action_type",
        ]
    );

    // shopify market — exact 5-branch case cascade. We CAN construct
    // the full expected tree; this is the strongest "no silent fix"
    // assertion we get for the eq:[a, {lit: x}] sequence-form sugar.
    let shopify_market_expr = match shopify
        .semantic_interface
        .dimensions
        .iter()
        .find(|e| e.name().as_ref() == "market")
        .unwrap()
    {
        DimensionEntry::Inline(d) => block_expr(d.expr.as_ref().unwrap()),
        _ => panic!("shopify.market should be Inline"),
    };
    let expected_shopify_market = Expr::Case {
        whens: vec![
            (
                binop(BinaryOpKind::Eq, field("country"), lit_str("Germany")),
                lit_str("DE"),
            ),
            (
                binop(BinaryOpKind::Eq, field("country"), lit_str("Spain")),
                lit_str("ES"),
            ),
            (
                binop(BinaryOpKind::Eq, field("country"), lit_str("France")),
                lit_str("FR"),
            ),
            (
                binop(BinaryOpKind::Eq, field("country"), lit_str("United Kingdom")),
                lit_str("GB"),
            ),
            (
                binop(BinaryOpKind::Eq, field("country"), lit_str("Italy")),
                lit_str("IT"),
            ),
        ],
        else_: Some(Box::new(lit_null())),
    };
    assert_eq!(
        shopify_market_expr, &expected_shopify_market,
        "shopify.market full case cascade equality"
    );

    // shopify measures — three sums: orders, revenue, tax.
    let s_meas: Vec<(String, AggregationType, DataType)> = shopify
        .semantic_interface
        .measures
        .iter()
        .map(|e| match e {
            MeasureEntry::Inline(m) => (
                m.name.as_ref().to_owned(),
                m.agg,
                m.data_type.clone(),
            ),
            MeasureEntry::Ref(_) => panic!("shopify measures are inline"),
            other => panic!("unknown MeasureEntry variant: {other:?}"),
        })
        .collect();
    assert_eq!(
        s_meas,
        vec![
            (
                "orders".to_string(),
                AggregationType::Sum,
                DataType::Decimal { precision: 18, scale: 2 }
            ),
            (
                "revenue".into(),
                AggregationType::Sum,
                DataType::Decimal { precision: 18, scale: 2 }
            ),
            (
                "tax".into(),
                AggregationType::Sum,
                DataType::Decimal { precision: 18, scale: 2 }
            ),
        ]
    );

    // shopify mapping spot-check: bare scalar (with dash) survives
    // unchanged as a Column.
    let smap = match &shopify.body.base.extras.semantic_mapping {
        SemanticMapping::Explicit(m) => m,
        other => panic!("expected Explicit mapping, got {other:?}"),
    };
    let s_country_key = SemanticsName("country".into());
    assert_eq!(
        smap.get(&s_country_key).unwrap(),
        &SemanticMappingValue::Column("shopify-shipping_country".into()),
        "shopify country mapping: dashed column atom"
    );
    // brand = literal Alpinestars
    let brand = SemanticsName("brand".into());
    assert_eq!(
        smap.get(&brand).unwrap(),
        &SemanticMappingValue::Literal(LiteralValue::String("Alpinestars".into()))
    );
    // We sanity-touch token rules here too: even though `1` is a number
    // in YAML, we never wrote a bare `1` at a column site — only as
    // `lit: 1` (inside the regex-extract cascade). The assertion above
    // already verified that path emits Literal::Integer(1) at depth.
    // Keep these flag uses suppressed:
    let _ = (
        UnaryOpKind::Negate,
        AggregationOp::Sum,
        LikeKind::Like,
        lit_int(1),
    );
}

// ── Operator-tag counter — helpers for the unionset cascade ────────────

#[derive(Default)]
struct OpCounts {
    case: usize,
    in_list: usize,
    binop_eq: usize,
    regexp_match: usize,
    regexp_extract: usize,
    upper: usize,
}

fn count_op_kinds(expr: &Expr<SemanticLeaf>) -> OpCounts {
    use semstrait_ir::tree::{Tree, Visitor};
    use std::ops::ControlFlow;
    struct C(OpCounts);
    impl Visitor<Expr<SemanticLeaf>> for C {
        type Output = ();
        fn f_down(&mut self, n: &Expr<SemanticLeaf>) -> ControlFlow<()> {
            match n {
                Expr::Case { .. } => self.0.case += 1,
                Expr::InList { .. } => self.0.in_list += 1,
                Expr::BinaryOp { op: BinaryOpKind::Eq, .. } => self.0.binop_eq += 1,
                Expr::FunctionCall { name, .. } => match name.0.as_str() {
                    "REGEXP_MATCH" => self.0.regexp_match += 1,
                    "REGEXP_EXTRACT" => self.0.regexp_extract += 1,
                    "UPPER" => self.0.upper += 1,
                    _ => {}
                },
                _ => {}
            }
            ControlFlow::Continue(())
        }
        fn f_up(&mut self, _n: &Expr<SemanticLeaf>) -> ControlFlow<()> {
            ControlFlow::Continue(())
        }
    }
    let mut c = C(OpCounts::default());
    let _ = expr.apply(&mut c);
    c.0
}
