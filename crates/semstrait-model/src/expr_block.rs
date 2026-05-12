//! Declarative YAML expression blocks -> Expr conversion.
//!
//! Provides `ExprSource` (inline DSL string or declarative YAML block)
//! and `ExprBlock` (structured YAML that maps 1:1 to `Expr` variants).

use semstrait_core::expr::{self, Expr, WhenClause};
use semstrait_core::Grain;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;

// ─── ExprSource ─────────────────────────────────────────────────────────────

/// Expression source in YAML — discriminated by serde type.
///
/// - `String` value routes to inline DSL parser (existing `parse_expr`)
/// - `Map` value deserializes directly as a declarative `ExprBlock`
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ExprSource {
    /// Simple string DSL: "cost / clicks", "{{ revenue }}", "amount"
    Inline(String),
    /// Declarative tree: maps directly to Expr via ExprBlock
    Declarative(ExprBlock),
}

impl<'de> Deserialize<'de> for ExprSource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        match &value {
            Value::String(s) => Ok(ExprSource::Inline(s.clone())),
            Value::Mapping(_) => {
                let block: ExprBlock = serde_yaml::from_value(value)
                    .map_err(serde::de::Error::custom)?;
                Ok(ExprSource::Declarative(block))
            }
            _ => Err(serde::de::Error::custom(
                "ExprSource: expected a string or mapping",
            )),
        }
    }
}

impl ExprSource {
    /// Returns the inline DSL string, if this is an inline source.
    pub fn as_inline_str(&self) -> Option<&str> {
        match self {
            ExprSource::Inline(s) => Some(s),
            ExprSource::Declarative(_) => None,
        }
    }

    /// Human-readable representation for debugging / compiled artifacts.
    pub fn display_string(&self) -> String {
        match self {
            ExprSource::Inline(s) => s.clone(),
            ExprSource::Declarative(_) => "<declarative>".to_string(),
        }
    }
}

// ─── ExprBlock ──────────────────────────────────────────────────────────────

/// Declarative expression block — YAML expressions that map to `Expr` variants.
///
/// Bare strings are column references, bare numbers/bools/null are literals.
/// Tagged forms use single-key maps: `{lit: "value"}`, `{upper: col}`, `{in: [col, v1, v2]}`.
/// Struct blocks support both array-positional and named-map forms.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprBlock {
    // ── Leaf nodes ──────────────────────────────────────────────
    /// Explicit column reference.
    Column(String),
    /// Explicit literal value.
    Literal(LiteralValue),

    // ── Arithmetic (BinaryOp) ──────────────────────────────────
    Add(TwoArgs),
    Subtract(TwoArgs),
    Multiply(TwoArgs),
    Divide(TwoArgs),
    SafeDivide(TwoArgs),

    // ── Comparison (BinaryOp) ──────────────────────────────────
    Eq(TwoArgs),
    NotEq(TwoArgs),
    Lt(TwoArgs),
    Gt(TwoArgs),
    Lte(TwoArgs),
    Gte(TwoArgs),

    // ── Logical ────────────────────────────────────────────────
    And(TwoArgs),
    Or(TwoArgs),
    Not(Box<ExprBlock>),
    Negate(Box<ExprBlock>),

    // ── Conditional ────────────────────────────────────────────
    Case(CaseBlock),
    Coalesce(Vec<ExprBlock>),
    NullIf(NullIfBlock),
    If(ThreeArgs),
    Greatest(Vec<ExprBlock>),
    Least(Vec<ExprBlock>),

    // ── Predicates ─────────────────────────────────────────────
    InList(InListBlock),
    NotInList(InListBlock),
    Between(BetweenBlock),
    Like(PatternBlock),
    Ilike(PatternBlock),
    IsNull(Box<ExprBlock>),
    IsNotNull(Box<ExprBlock>),

    // ── Pattern matching ───────────────────────────────────────
    RegexpMatch(RegexpMatchBlock),
    RegexpExtract(RegexpExtractBlock),
    RegexpReplace(RegexpReplaceBlock),

    // ── String functions ───────────────────────────────────────
    Upper(Box<ExprBlock>),
    Lower(Box<ExprBlock>),
    Trim(Box<ExprBlock>),
    Ltrim(Box<ExprBlock>),
    Rtrim(Box<ExprBlock>),
    Length(Box<ExprBlock>),
    Reverse(Box<ExprBlock>),
    Initcap(Box<ExprBlock>),
    Concat(Vec<ExprBlock>),
    ConcatWs(ConcatWsBlock),
    Replace(ReplaceBlock),
    Substring(SubstringBlock),
    Left(LeftRightBlock),
    Right(LeftRightBlock),
    Repeat(LeftRightBlock),
    Lpad(PadBlock),
    Rpad(PadBlock),
    StartsWith(PatternBlock),
    EndsWith(PatternBlock),
    Position(PatternBlock),
    SplitPart(SplitPartBlock),

    // ── Math functions ─────────────────────────────────────────
    Abs(Box<ExprBlock>),
    Ceil(Box<ExprBlock>),
    Floor(Box<ExprBlock>),
    Round(RoundBlock),
    Power(PowerBlock),
    Sqrt(Box<ExprBlock>),
    Mod(TwoArgs),

    // ── Date functions ─────────────────────────────────────────
    DateTrunc(DateTruncBlock),
    CurrentDate(EmptyBlock),
    CurrentTimestamp(EmptyBlock),
    DateAdd(DateAddBlock),
    DateDiff(DateDiffBlock),
    Extract(ExtractBlock),
    ToDate(ToDateBlock),
    ToTimestamp(ToDateBlock),

    // ── Type conversion ────────────────────────────────────────
    Cast(CastBlock),

    // ── Guard (sugar) ──────────────────────────────────────────
    Guard(GuardBlock),
}

// ─── Supporting types ───────────────────────────────────────────────────────
//
// All struct blocks support BOTH forms:
//   - Array positional: `in: [col, val1, val2]` (new, concise)
//   - Named map: `in: {col: x, list: [...]}` (backward compat; `expr:` accepted as alias for `col:`)

/// Two-argument expression (used for binary ops and two-arg functions).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TwoArgs {
    /// Array form: `add: [left, right]`
    Array([Box<ExprBlock>; 2]),
    /// Map form with named fields
    Map { left: Box<ExprBlock>, right: Box<ExprBlock> },
}

/// Three-argument expression (used for IF).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreeArgs(pub [Box<ExprBlock>; 3]);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaseBlock {
    pub when: Vec<WhenBlock>,
    #[serde(default, rename = "else")]
    pub else_expr: Option<Box<ExprBlock>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WhenBlock {
    pub condition: ExprBlock,
    pub then: ExprBlock,
}

// ── Helper: extract ExprBlock from Value (used by array-form deserializers) ──

fn expr_from_val(v: &Value) -> Result<ExprBlock, String> {
    from_val::<ExprBlock>(v.clone())
}

fn i64_from_val(v: &Value) -> Result<i64, String> {
    v.as_i64()
        .or_else(|| v.as_f64().map(|f| f as i64))
        .ok_or_else(|| format!("expected integer, got {:?}", v))
}

fn usize_from_val(v: &Value) -> Result<usize, String> {
    i64_from_val(v).map(|i| i as usize)
}

/// Extracts a string from a Value that may be a bare string OR a `{lit: "..."}` ExprBlock.
fn string_from_expr_val(v: &Value) -> Result<String, String> {
    // Try bare string first
    if let Some(s) = v.as_str() {
        return Ok(s.to_string());
    }
    // Try as ExprBlock — must resolve to a literal string
    let block = expr_from_val(v)?;
    match &block {
        ExprBlock::Literal(LiteralValue::String(s)) => Ok(s.clone()),
        _ => Err(format!("expected string literal, got {:?}", block)),
    }
}

fn seq_min<'a>(v: &'a Value, min: usize, name: &str) -> Result<&'a Vec<Value>, String> {
    let seq = v.as_sequence().ok_or_else(|| format!("{name}: expected array"))?;
    if seq.len() < min {
        return Err(format!("{name}: expected at least {min} elements, got {}", seq.len()));
    }
    Ok(seq)
}

// ── Macro for dual-form (array + map) deserialization ────────────────────────

/// Generates a custom Deserialize that tries array form first, then falls back to map form.
/// The map form uses a private helper struct with #[derive(Deserialize)].
macro_rules! dual_deser {
    // Pattern: struct with array parser function
    ($name:ident, $array_fn:expr, { $( $(#[$meta:meta])* $field:ident : $fty:ty ),* $(,)? }) => {
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where D: serde::Deserializer<'de>,
            {
                let value = Value::deserialize(deserializer)?;
                if value.is_sequence() {
                    return ($array_fn)(&value).map_err(serde::de::Error::custom);
                }
                // Fall back to named map form
                #[derive(Deserialize)]
                struct Map {
                    $( $(#[$meta])* $field : $fty, )*
                }
                let m: Map = from_val(value).map_err(serde::de::Error::custom)?;
                Ok($name { $( $field: m.$field, )* })
            }
        }
    };
}

// ── InListBlock ──────────────────────────────────────────────────────────────
// Array: `in: [col, val1, val2, ...]`    Map: `in: {col: x, list: [...]}`

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InListBlock {
    pub col: Box<ExprBlock>,
    pub list: Vec<ExprBlock>,
}

dual_deser!(InListBlock, |v: &Value| -> Result<InListBlock, String> {
    let seq = seq_min(v, 2, "in")?;
    let col = expr_from_val(&seq[0])?;
    let list = seq[1..].iter().map(expr_from_val).collect::<Result<Vec<_>, _>>()?;
    Ok(InListBlock { col: Box::new(col), list })
}, {
    col: Box<ExprBlock>,
    list: Vec<ExprBlock>,
});

// ── BetweenBlock ─────────────────────────────────────────────────────────────
// Array: `between: [col, low, high]`

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BetweenBlock {
    pub col: Box<ExprBlock>,
    pub low: Box<ExprBlock>,
    pub high: Box<ExprBlock>,
}

dual_deser!(BetweenBlock, |v: &Value| -> Result<BetweenBlock, String> {
    let seq = seq_min(v, 3, "between")?;
    Ok(BetweenBlock {
        col: Box::new(expr_from_val(&seq[0])?),
        low: Box::new(expr_from_val(&seq[1])?),
        high: Box::new(expr_from_val(&seq[2])?),
    })
}, {
    col: Box<ExprBlock>,
    low: Box<ExprBlock>,
    high: Box<ExprBlock>,
});

// ── PatternBlock ─────────────────────────────────────────────────────────────
// Array: `like: [col, pattern]`

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PatternBlock {
    pub col: Box<ExprBlock>,
    pub pattern: Box<ExprBlock>,
}

dual_deser!(PatternBlock, |v: &Value| -> Result<PatternBlock, String> {
    let seq = seq_min(v, 2, "like/ilike")?;
    Ok(PatternBlock {
        col: Box::new(expr_from_val(&seq[0])?),
        pattern: Box::new(expr_from_val(&seq[1])?),
    })
}, {
    col: Box<ExprBlock>,
    pattern: Box<ExprBlock>,
});

// ── NullIfBlock ──────────────────────────────────────────────────────────────
// Array: `nullif: [col, value]`

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NullIfBlock {
    pub col: Box<ExprBlock>,
    pub null_expr: Box<ExprBlock>,
}

dual_deser!(NullIfBlock, |v: &Value| -> Result<NullIfBlock, String> {
    let seq = seq_min(v, 2, "nullif")?;
    Ok(NullIfBlock {
        col: Box::new(expr_from_val(&seq[0])?),
        null_expr: Box::new(expr_from_val(&seq[1])?),
    })
}, {
    col: Box<ExprBlock>,
    null_expr: Box<ExprBlock>,
});

// ── RegexpMatchBlock ─────────────────────────────────────────────────────────
// Array: `regexp_match: [col, pattern]`   Map: `regexp_match: {col:, pattern:, full_match:}`

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RegexpMatchBlock {
    pub col: Box<ExprBlock>,
    pub pattern: Box<ExprBlock>,
    pub full_match: bool,
}

dual_deser!(RegexpMatchBlock, |v: &Value| -> Result<RegexpMatchBlock, String> {
    let seq = seq_min(v, 2, "regexp_match")?;
    Ok(RegexpMatchBlock {
        col: Box::new(expr_from_val(&seq[0])?),
        pattern: Box::new(expr_from_val(&seq[1])?),
        full_match: false,
    })
}, {
    col: Box<ExprBlock>,
    pattern: Box<ExprBlock>,
    #[serde(default)] full_match: bool,
});

// ── RegexpExtractBlock ───────────────────────────────────────────────────────
// Array: `regexp_extract: [col, pattern, group?]`

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RegexpExtractBlock {
    pub col: Box<ExprBlock>,
    pub pattern: Box<ExprBlock>,
    pub group: usize,
}

dual_deser!(RegexpExtractBlock, |v: &Value| -> Result<RegexpExtractBlock, String> {
    let seq = seq_min(v, 2, "regexp_extract")?;
    let group = if seq.len() > 2 { usize_from_val(&seq[2])? } else { 0 };
    Ok(RegexpExtractBlock {
        col: Box::new(expr_from_val(&seq[0])?),
        pattern: Box::new(expr_from_val(&seq[1])?),
        group,
    })
}, {
    col: Box<ExprBlock>,
    pattern: Box<ExprBlock>,
    #[serde(default)] group: usize,
});

// ── RegexpReplaceBlock ──────────────────────────────────────────────────────
// Array: `regexp_replace: [col, pattern, replacement]`

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RegexpReplaceBlock {
    pub col: Box<ExprBlock>,
    pub pattern: Box<ExprBlock>,
    pub replacement: Box<ExprBlock>,
}

dual_deser!(RegexpReplaceBlock, |v: &Value| -> Result<RegexpReplaceBlock, String> {
    let seq = seq_min(v, 3, "regexp_replace")?;
    Ok(RegexpReplaceBlock {
        col: Box::new(expr_from_val(&seq[0])?),
        pattern: Box::new(expr_from_val(&seq[1])?),
        replacement: Box::new(expr_from_val(&seq[2])?),
    })
}, {
    col: Box<ExprBlock>,
    pattern: Box<ExprBlock>,
    replacement: Box<ExprBlock>,
});

// ── ReplaceBlock ─────────────────────────────────────────────────────────────
// Array: `replace: [col, old, new]`

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReplaceBlock {
    pub col: Box<ExprBlock>,
    pub old: Box<ExprBlock>,
    pub new: Box<ExprBlock>,
}

dual_deser!(ReplaceBlock, |v: &Value| -> Result<ReplaceBlock, String> {
    let seq = seq_min(v, 3, "replace")?;
    Ok(ReplaceBlock {
        col: Box::new(expr_from_val(&seq[0])?),
        old: Box::new(expr_from_val(&seq[1])?),
        new: Box::new(expr_from_val(&seq[2])?),
    })
}, {
    col: Box<ExprBlock>,
    old: Box<ExprBlock>,
    new: Box<ExprBlock>,
});

// ── SplitPartBlock ──────────────────────────────────────────────────────────
// Array: `split_part: [col, delimiter, part]`

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SplitPartBlock {
    pub col: Box<ExprBlock>,
    pub delimiter: Box<ExprBlock>,
    pub part: i64,
}

dual_deser!(SplitPartBlock, |v: &Value| -> Result<SplitPartBlock, String> {
    let seq = seq_min(v, 3, "split_part")?;
    Ok(SplitPartBlock {
        col: Box::new(expr_from_val(&seq[0])?),
        delimiter: Box::new(expr_from_val(&seq[1])?),
        part: i64_from_val(&seq[2])?,
    })
}, {
    col: Box<ExprBlock>,
    delimiter: Box<ExprBlock>,
    part: i64,
});

// ── ConcatWsBlock ───────────────────────────────────────────────────────────
// Array: `concat_ws: [separator, expr1, expr2, ...]`
// Map: `concat_ws: {separator: ..., exprs: [...]}`

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConcatWsBlock {
    pub separator: Box<ExprBlock>,
    pub exprs: Vec<ExprBlock>,
}

dual_deser!(ConcatWsBlock, |v: &Value| -> Result<ConcatWsBlock, String> {
    let seq = seq_min(v, 2, "concat_ws")?;
    let separator = expr_from_val(&seq[0])?;
    let exprs = seq[1..].iter().map(expr_from_val).collect::<Result<Vec<_>, _>>()?;
    Ok(ConcatWsBlock { separator: Box::new(separator), exprs })
}, {
    separator: Box<ExprBlock>,
    exprs: Vec<ExprBlock>,
});

// ── SubstringBlock ───────────────────────────────────────────────────────────
// Array: `substr: [col, start, length?]`

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SubstringBlock {
    pub col: Box<ExprBlock>,
    pub start: i64,
    pub length: Option<i64>,
}

dual_deser!(SubstringBlock, |v: &Value| -> Result<SubstringBlock, String> {
    let seq = seq_min(v, 2, "substr")?;
    let length = if seq.len() > 2 { Some(i64_from_val(&seq[2])?) } else { None };
    Ok(SubstringBlock {
        col: Box::new(expr_from_val(&seq[0])?),
        start: i64_from_val(&seq[1])?,
        length,
    })
}, {
    col: Box<ExprBlock>,
    start: i64,
    #[serde(default)] length: Option<i64>,
});

// ── LeftRightBlock ───────────────────────────────────────────────────────────
// Array: `left: [col, length]`

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LeftRightBlock {
    pub col: Box<ExprBlock>,
    pub length: i64,
}

dual_deser!(LeftRightBlock, |v: &Value| -> Result<LeftRightBlock, String> {
    let seq = seq_min(v, 2, "left/right")?;
    Ok(LeftRightBlock {
        col: Box::new(expr_from_val(&seq[0])?),
        length: i64_from_val(&seq[1])?,
    })
}, {
    col: Box<ExprBlock>,
    length: i64,
});

// ── PadBlock ─────────────────────────────────────────────────────────────────
// Array: `lpad: [col, length, fill?]`

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PadBlock {
    pub col: Box<ExprBlock>,
    pub length: i64,
    pub fill: String,
}

fn default_pad_fill() -> String {
    " ".to_string()
}

dual_deser!(PadBlock, |v: &Value| -> Result<PadBlock, String> {
    let seq = seq_min(v, 2, "lpad/rpad")?;
    let fill = if seq.len() > 2 { string_from_expr_val(&seq[2])? } else { " ".to_string() };
    Ok(PadBlock {
        col: Box::new(expr_from_val(&seq[0])?),
        length: i64_from_val(&seq[1])?,
        fill,
    })
}, {
    col: Box<ExprBlock>,
    length: i64,
    #[serde(default = "default_pad_fill")] fill: String,
});

// ── RoundBlock ───────────────────────────────────────────────────────────────
// Array: `round: [col, scale?]`   Single: `round: col`

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoundBlock {
    pub col: Box<ExprBlock>,
    pub scale: i64,
}

dual_deser!(RoundBlock, |v: &Value| -> Result<RoundBlock, String> {
    let seq = seq_min(v, 1, "round")?;
    let scale = if seq.len() > 1 { i64_from_val(&seq[1])? } else { 0 };
    Ok(RoundBlock {
        col: Box::new(expr_from_val(&seq[0])?),
        scale,
    })
}, {
    col: Box<ExprBlock>,
    #[serde(default)] scale: i64,
});

// ── PowerBlock ───────────────────────────────────────────────────────────────
// Array: `power: [base, exponent]`

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PowerBlock {
    pub base: Box<ExprBlock>,
    pub exponent: Box<ExprBlock>,
}

dual_deser!(PowerBlock, |v: &Value| -> Result<PowerBlock, String> {
    let seq = seq_min(v, 2, "power")?;
    Ok(PowerBlock {
        base: Box::new(expr_from_val(&seq[0])?),
        exponent: Box::new(expr_from_val(&seq[1])?),
    })
}, {
    base: Box<ExprBlock>,
    exponent: Box<ExprBlock>,
});

// ── DateTruncBlock ───────────────────────────────────────────────────────────
// Array: `date_trunc: [grain, col]`

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DateTruncBlock {
    pub grain: String,
    pub col: Box<ExprBlock>,
}

dual_deser!(DateTruncBlock, |v: &Value| -> Result<DateTruncBlock, String> {
    let seq = seq_min(v, 2, "date_trunc")?;
    Ok(DateTruncBlock {
        grain: string_from_expr_val(&seq[0])?,
        col: Box::new(expr_from_val(&seq[1])?),
    })
}, {
    grain: String,
    col: Box<ExprBlock>,
});

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmptyBlock {}

// ── DateAddBlock ─────────────────────────────────────────────────────────────
// Array: `date_add: [col, days]`

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DateAddBlock {
    pub col: Box<ExprBlock>,
    pub days: i64,
}

dual_deser!(DateAddBlock, |v: &Value| -> Result<DateAddBlock, String> {
    let seq = seq_min(v, 2, "date_add")?;
    Ok(DateAddBlock {
        col: Box::new(expr_from_val(&seq[0])?),
        days: i64_from_val(&seq[1])?,
    })
}, {
    col: Box<ExprBlock>,
    days: i64,
});

// ── DateDiffBlock ────────────────────────────────────────────────────────────
// Array: `date_diff: [start, end]`

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DateDiffBlock {
    pub start: Box<ExprBlock>,
    pub end: Box<ExprBlock>,
}

dual_deser!(DateDiffBlock, |v: &Value| -> Result<DateDiffBlock, String> {
    let seq = seq_min(v, 2, "date_diff")?;
    Ok(DateDiffBlock {
        start: Box::new(expr_from_val(&seq[0])?),
        end: Box::new(expr_from_val(&seq[1])?),
    })
}, {
    start: Box<ExprBlock>,
    end: Box<ExprBlock>,
});

// ── ToDateBlock ─────────────────────────────────────────────────────────────
// Array: `to_date: [col]` or `to_date: [col, format]`
// Also reused for `to_timestamp`.

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToDateBlock {
    pub col: Box<ExprBlock>,
    pub format: Option<Box<ExprBlock>>,
}

dual_deser!(ToDateBlock, |v: &Value| -> Result<ToDateBlock, String> {
    let seq = seq_min(v, 1, "to_date/to_timestamp")?;
    let format = if seq.len() > 1 { Some(Box::new(expr_from_val(&seq[1])?)) } else { None };
    Ok(ToDateBlock {
        col: Box::new(expr_from_val(&seq[0])?),
        format,
    })
}, {
    col: Box<ExprBlock>,
    #[serde(default)] format: Option<Box<ExprBlock>>,
});

// ── ExtractBlock ─────────────────────────────────────────────────────────────
// Array: `extract: [part, col]`

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExtractBlock {
    pub part: String,
    pub col: Box<ExprBlock>,
}

dual_deser!(ExtractBlock, |v: &Value| -> Result<ExtractBlock, String> {
    let seq = seq_min(v, 2, "extract")?;
    Ok(ExtractBlock {
        part: string_from_expr_val(&seq[0])?,
        col: Box::new(expr_from_val(&seq[1])?),
    })
}, {
    part: String,
    col: Box<ExprBlock>,
});

// ── CastBlock ────────────────────────────────────────────────────────────────
// Array: `cast: [col, type]`

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CastBlock {
    pub col: Box<ExprBlock>,
    pub to: String,
}

dual_deser!(CastBlock, |v: &Value| -> Result<CastBlock, String> {
    let seq = seq_min(v, 2, "cast")?;
    Ok(CastBlock {
        col: Box::new(expr_from_val(&seq[0])?),
        to: string_from_expr_val(&seq[1])?,
    })
}, {
    col: Box<ExprBlock>,
    to: String,
});

// ── GuardBlock ───────────────────────────────────────────────────────────────
// Array: `guard: [condition, col]`

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GuardBlock {
    pub condition: Box<ExprBlock>,
    pub col: Box<ExprBlock>,
}

dual_deser!(GuardBlock, |v: &Value| -> Result<GuardBlock, String> {
    let seq = seq_min(v, 2, "guard")?;
    Ok(GuardBlock {
        condition: Box::new(expr_from_val(&seq[0])?),
        col: Box::new(expr_from_val(&seq[1])?),
    })
}, {
    condition: Box<ExprBlock>,
    col: Box<ExprBlock>,
});

// ─── Custom Serialize / Deserialize for ExprBlock ──────────────────────────
//
// serde_yaml 0.9 uses YAML tags (`!variant`) for externally-tagged enums.
// We want: bare scalars, single-key map syntax (`{lit: "amount"}`, `{add: [a, b]}`),
// and array-positional forms (`in: [col, val1, val2]`).

/// Helper: deserialize a `serde_yaml::Value` into `T`.
fn from_val<T: serde::de::DeserializeOwned>(v: Value) -> Result<T, String> {
    serde_yaml::from_value(v).map_err(|e| e.to_string())
}

/// Maps variant-name strings ↔ ExprBlock variants for serde.
macro_rules! expr_block_serde {
    ( $( $key:literal $( | $alias:literal )* => $variant:ident ( $ty:ty ) ),* $(,)? ) => {
        impl Serialize for ExprBlock {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where S: serde::Serializer,
            {
                // Column serializes as bare string (no tag)
                if let ExprBlock::Column(name) = self {
                    return serializer.serialize_str(name);
                }
                use serde::ser::SerializeMap;
                let mut map = serializer.serialize_map(Some(1))?;
                match self {
                    ExprBlock::Column(_) => unreachable!(),
                    $( ExprBlock::$variant(v) => map.serialize_entry($key, v)?, )*
                }
                map.end()
            }
        }

        impl<'de> Deserialize<'de> for ExprBlock {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where D: serde::Deserializer<'de>,
            {
                let value = Value::deserialize(deserializer)?;

                // ── Bare scalar sugar ───────────────────────────────────
                // string → Column ref, number/bool/null → Literal
                match &value {
                    Value::String(s) => return Ok(ExprBlock::Column(s.clone())),
                    Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            return Ok(ExprBlock::Literal(LiteralValue::Integer(i)));
                        } else if let Some(f) = n.as_f64() {
                            return Ok(ExprBlock::Literal(LiteralValue::Float(f)));
                        }
                    }
                    Value::Bool(b) => return Ok(ExprBlock::Literal(LiteralValue::Boolean(*b))),
                    Value::Null => return Ok(ExprBlock::Literal(LiteralValue::Null)),
                    _ => {}
                }

                // ── Single-key map (tagged expression) ──────────────────
                let mapping = value.as_mapping().ok_or_else(|| {
                    serde::de::Error::custom("ExprBlock: expected scalar, single-key map, or array")
                })?;
                if mapping.len() != 1 {
                    return Err(serde::de::Error::custom(format!(
                        "ExprBlock must have exactly 1 key, found {}",
                        mapping.len()
                    )));
                }
                let (key, val) = mapping.into_iter().next().unwrap();
                let key_str = key.as_str().ok_or_else(|| {
                    serde::de::Error::custom("ExprBlock key must be a string")
                })?;
                match key_str {
                    $( $key $( | $alias )* => from_val::<$ty>(val.clone())
                        .map(ExprBlock::$variant)
                        .map_err(serde::de::Error::custom), )*
                    other => Err(serde::de::Error::custom(
                        format!("unknown ExprBlock key: '{other}'")
                    )),
                }
            }
        }
    };
}

expr_block_serde! {
    // Leaf (bare string = column, bare number/bool/null = literal)
    "lit" => Literal(LiteralValue),
    // Arithmetic
    "add" => Add(TwoArgs),
    "subtract" => Subtract(TwoArgs),
    "multiply" => Multiply(TwoArgs),
    "divide" => Divide(TwoArgs),
    "safe_divide" => SafeDivide(TwoArgs),
    // Comparison
    "eq" => Eq(TwoArgs),
    "not_eq" => NotEq(TwoArgs),
    "lt" => Lt(TwoArgs),
    "gt" => Gt(TwoArgs),
    "lte" => Lte(TwoArgs),
    "gte" => Gte(TwoArgs),
    // Logical
    "and" => And(TwoArgs),
    "or" => Or(TwoArgs),
    "not" => Not(Box<ExprBlock>),
    "negate" => Negate(Box<ExprBlock>),
    // Conditional
    "case" => Case(CaseBlock),
    "coalesce" => Coalesce(Vec<ExprBlock>),
    "nullif" => NullIf(NullIfBlock),
    "if" => If(ThreeArgs),
    "greatest" => Greatest(Vec<ExprBlock>),
    "least" => Least(Vec<ExprBlock>),
    // Predicates
    "in" => InList(InListBlock),
    "not_in" => NotInList(InListBlock),
    "between" => Between(BetweenBlock),
    "like" => Like(PatternBlock),
    "ilike" => Ilike(PatternBlock),
    "is_null" => IsNull(Box<ExprBlock>),
    "is_not_null" => IsNotNull(Box<ExprBlock>),
    // Pattern matching
    "regexp_match" => RegexpMatch(RegexpMatchBlock),
    "regexp_extract" => RegexpExtract(RegexpExtractBlock),
    "regexp_replace" => RegexpReplace(RegexpReplaceBlock),
    // String functions
    "upper" => Upper(Box<ExprBlock>),
    "lower" => Lower(Box<ExprBlock>),
    "trim" => Trim(Box<ExprBlock>),
    "ltrim" => Ltrim(Box<ExprBlock>),
    "rtrim" => Rtrim(Box<ExprBlock>),
    "length" => Length(Box<ExprBlock>),
    "reverse" => Reverse(Box<ExprBlock>),
    "initcap" => Initcap(Box<ExprBlock>),
    "concat" => Concat(Vec<ExprBlock>),
    "concat_ws" => ConcatWs(ConcatWsBlock),
    "replace" => Replace(ReplaceBlock),
    "substr" => Substring(SubstringBlock),
    "left" => Left(LeftRightBlock),
    "right" => Right(LeftRightBlock),
    "repeat" => Repeat(LeftRightBlock),
    "lpad" => Lpad(PadBlock),
    "rpad" => Rpad(PadBlock),
    "starts_with" => StartsWith(PatternBlock),
    "ends_with" => EndsWith(PatternBlock),
    "position" => Position(PatternBlock),
    "split_part" => SplitPart(SplitPartBlock),
    // Math functions
    "abs" => Abs(Box<ExprBlock>),
    "ceil" => Ceil(Box<ExprBlock>),
    "floor" => Floor(Box<ExprBlock>),
    "round" => Round(RoundBlock),
    "power" => Power(PowerBlock),
    "sqrt" => Sqrt(Box<ExprBlock>),
    "mod" => Mod(TwoArgs),
    // Date functions
    "date_trunc" => DateTrunc(DateTruncBlock),
    "current_date" => CurrentDate(EmptyBlock),
    "current_timestamp" => CurrentTimestamp(EmptyBlock),
    "date_add" => DateAdd(DateAddBlock),
    "date_diff" => DateDiff(DateDiffBlock),
    "extract" => Extract(ExtractBlock),
    "to_date" => ToDate(ToDateBlock),
    "to_timestamp" => ToTimestamp(ToDateBlock),
    // Type conversion
    "cast" => Cast(CastBlock),
    // Guard
    "guard" => Guard(GuardBlock),
}

/// Literal value for explicit `literal:` key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LiteralValue {
    Integer(i64),
    Float(f64),
    String(String),
    Boolean(bool),
    Null,
}

// ─── ExprBlock -> Expr conversion ───────────────────────────────────────────

impl ExprBlock {
    /// Convert a declarative ExprBlock to a core Expr.
    ///
    /// Leaf strings are converted to `EntityRef` (resolved during compilation).
    /// Standard functions become `Expr::FunctionCall`.
    pub fn to_expr(&self) -> Result<Expr, ExprBlockError> {
        match self {
            // ── Leaf nodes ──────────────────────────────────────
            ExprBlock::Column(name) => Ok(Expr::column(name.clone())),
            ExprBlock::Literal(lit) => Ok(lit.to_expr()),

            // ── Arithmetic ──────────────────────────────────────
            ExprBlock::Add(args) => binary(args, expr::BinaryOp::Add),
            ExprBlock::Subtract(args) => binary(args, expr::BinaryOp::Subtract),
            ExprBlock::Multiply(args) => binary(args, expr::BinaryOp::Multiply),
            ExprBlock::Divide(args) => binary(args, expr::BinaryOp::Divide),
            ExprBlock::SafeDivide(args) => binary(args, expr::BinaryOp::SafeDivide),

            // ── Comparison ──────────────────────────────────────
            ExprBlock::Eq(args) => binary(args, expr::BinaryOp::Eq),
            ExprBlock::NotEq(args) => binary(args, expr::BinaryOp::NotEq),
            ExprBlock::Lt(args) => binary(args, expr::BinaryOp::Lt),
            ExprBlock::Gt(args) => binary(args, expr::BinaryOp::Gt),
            ExprBlock::Lte(args) => binary(args, expr::BinaryOp::LtEq),
            ExprBlock::Gte(args) => binary(args, expr::BinaryOp::GtEq),

            // ── Logical ─────────────────────────────────────────
            ExprBlock::And(args) => binary(args, expr::BinaryOp::And),
            ExprBlock::Or(args) => binary(args, expr::BinaryOp::Or),
            ExprBlock::Not(inner) => Ok(Expr::not(inner.to_expr()?)),
            ExprBlock::Negate(inner) => Ok(Expr::negate(inner.to_expr()?)),

            // ── Conditional ─────────────────────────────────────
            ExprBlock::Case(c) => {
                let when_then = c
                    .when
                    .iter()
                    .map(|w| {
                        Ok(WhenClause::new(
                            w.condition.to_expr()?,
                            w.then.to_expr()?,
                        ))
                    })
                    .collect::<Result<Vec<_>, ExprBlockError>>()?;
                let else_expr = c
                    .else_expr
                    .as_ref()
                    .map(|e| e.to_expr())
                    .transpose()?;
                Ok(Expr::case(when_then, else_expr))
            }
            ExprBlock::Coalesce(exprs) => {
                let args = convert_many(exprs)?;
                Ok(Expr::coalesce(args))
            }
            ExprBlock::NullIf(n) => {
                Ok(Expr::null_if(n.col.to_expr()?, n.null_expr.to_expr()?))
            }
            ExprBlock::If(ThreeArgs([cond, then_val, else_val])) => {
                Ok(Expr::case(
                    vec![WhenClause::new(cond.to_expr()?, then_val.to_expr()?)],
                    Some(else_val.to_expr()?),
                ))
            }
            ExprBlock::Greatest(exprs) => {
                let args = convert_many(exprs)?;
                Ok(Expr::function_call("GREATEST", args))
            }
            ExprBlock::Least(exprs) => {
                let args = convert_many(exprs)?;
                Ok(Expr::function_call("LEAST", args))
            }

            // ── Predicates ──────────────────────────────────────
            ExprBlock::InList(il) => {
                let list = convert_many(&il.list)?;
                Ok(Expr::in_list(il.col.to_expr()?, list))
            }
            ExprBlock::NotInList(il) => {
                let list = convert_many(&il.list)?;
                Ok(Expr::not_in_list(il.col.to_expr()?, list))
            }
            ExprBlock::Between(b) => {
                Ok(Expr::between(
                    b.col.to_expr()?,
                    b.low.to_expr()?,
                    b.high.to_expr()?,
                ))
            }
            ExprBlock::Like(p) => {
                Ok(Expr::like(p.col.to_expr()?, p.pattern.to_expr()?))
            }
            ExprBlock::Ilike(p) => {
                Ok(Expr::ilike(p.col.to_expr()?, p.pattern.to_expr()?))
            }
            ExprBlock::IsNull(inner) => Ok(Expr::is_null(inner.to_expr()?)),
            ExprBlock::IsNotNull(inner) => Ok(Expr::is_not_null(inner.to_expr()?)),

            // ── Pattern matching ────────────────────────────────
            ExprBlock::RegexpMatch(r) => {
                Ok(Expr::regexp_match(
                    r.col.to_expr()?,
                    r.pattern.to_expr()?,
                    r.full_match,
                ))
            }
            ExprBlock::RegexpExtract(r) => {
                Ok(Expr::regexp_extract(
                    r.col.to_expr()?,
                    r.pattern.to_expr()?,
                    r.group,
                ))
            }
            ExprBlock::RegexpReplace(r) => {
                Ok(Expr::function_call(
                    "REGEXP_REPLACE",
                    vec![r.col.to_expr()?, r.pattern.to_expr()?, r.replacement.to_expr()?],
                ))
            }

            // ── String functions → FunctionCall ─────────────────
            ExprBlock::Upper(e) => func1("UPPER", e),
            ExprBlock::Lower(e) => func1("LOWER", e),
            ExprBlock::Trim(e) => func1("TRIM", e),
            ExprBlock::Ltrim(e) => func1("LTRIM", e),
            ExprBlock::Rtrim(e) => func1("RTRIM", e),
            ExprBlock::Length(e) => func1("LENGTH", e),
            ExprBlock::Reverse(e) => func1("REVERSE", e),
            ExprBlock::Initcap(e) => func1("INITCAP", e),
            ExprBlock::Concat(exprs) => {
                let args = convert_many(exprs)?;
                Ok(Expr::function_call("CONCAT", args))
            }
            ExprBlock::ConcatWs(cw) => {
                let mut args = vec![cw.separator.to_expr()?];
                args.extend(convert_many(&cw.exprs)?);
                Ok(Expr::function_call("CONCAT_WS", args))
            }
            ExprBlock::Replace(r) => {
                Ok(Expr::function_call(
                    "REPLACE",
                    vec![r.col.to_expr()?, r.old.to_expr()?, r.new.to_expr()?],
                ))
            }
            ExprBlock::Substring(s) => {
                let mut args = vec![s.col.to_expr()?, Expr::int(s.start)];
                if let Some(len) = s.length {
                    args.push(Expr::int(len));
                }
                Ok(Expr::function_call("SUBSTRING", args))
            }
            ExprBlock::Left(lr) => {
                Ok(Expr::function_call(
                    "LEFT",
                    vec![lr.col.to_expr()?, Expr::int(lr.length)],
                ))
            }
            ExprBlock::Right(lr) => {
                Ok(Expr::function_call(
                    "RIGHT",
                    vec![lr.col.to_expr()?, Expr::int(lr.length)],
                ))
            }
            ExprBlock::Repeat(lr) => {
                Ok(Expr::function_call(
                    "REPEAT",
                    vec![lr.col.to_expr()?, Expr::int(lr.length)],
                ))
            }
            ExprBlock::Lpad(p) => {
                Ok(Expr::function_call(
                    "LPAD",
                    vec![p.col.to_expr()?, Expr::int(p.length), Expr::string(&p.fill)],
                ))
            }
            ExprBlock::Rpad(p) => {
                Ok(Expr::function_call(
                    "RPAD",
                    vec![p.col.to_expr()?, Expr::int(p.length), Expr::string(&p.fill)],
                ))
            }
            ExprBlock::StartsWith(p) => {
                Ok(Expr::function_call(
                    "STARTS_WITH",
                    vec![p.col.to_expr()?, p.pattern.to_expr()?],
                ))
            }
            ExprBlock::EndsWith(p) => {
                Ok(Expr::function_call(
                    "ENDS_WITH",
                    vec![p.col.to_expr()?, p.pattern.to_expr()?],
                ))
            }
            ExprBlock::Position(p) => {
                Ok(Expr::function_call(
                    "POSITION",
                    vec![p.col.to_expr()?, p.pattern.to_expr()?],
                ))
            }
            ExprBlock::SplitPart(sp) => {
                Ok(Expr::function_call(
                    "SPLIT_PART",
                    vec![sp.col.to_expr()?, sp.delimiter.to_expr()?, Expr::int(sp.part)],
                ))
            }

            // ── Math functions → FunctionCall ───────────────────
            ExprBlock::Abs(e) => func1("ABS", e),
            ExprBlock::Ceil(e) => func1("CEIL", e),
            ExprBlock::Floor(e) => func1("FLOOR", e),
            ExprBlock::Round(r) => {
                Ok(Expr::function_call(
                    "ROUND",
                    vec![r.col.to_expr()?, Expr::int(r.scale)],
                ))
            }
            ExprBlock::Power(p) => {
                Ok(Expr::function_call(
                    "POWER",
                    vec![p.base.to_expr()?, p.exponent.to_expr()?],
                ))
            }
            ExprBlock::Sqrt(e) => func1("SQRT", e),
            ExprBlock::Mod(args) => {
                let (l, r) = two_args(args)?;
                Ok(Expr::function_call("MOD", vec![l, r]))
            }

            // ── Date functions ──────────────────────────────────
            ExprBlock::DateTrunc(dt) => {
                let grain: Grain = dt
                    .grain
                    .parse()
                    .map_err(|_| ExprBlockError::InvalidGrain(dt.grain.clone()))?;
                Ok(Expr::date_trunc(grain, dt.col.to_expr()?))
            }
            ExprBlock::CurrentDate(_) => {
                Ok(Expr::function_call("CURRENT_DATE", vec![]))
            }
            ExprBlock::CurrentTimestamp(_) => {
                Ok(Expr::function_call("CURRENT_TIMESTAMP", vec![]))
            }
            ExprBlock::DateAdd(d) => {
                Ok(Expr::function_call(
                    "DATE_ADD",
                    vec![d.col.to_expr()?, Expr::int(d.days)],
                ))
            }
            ExprBlock::DateDiff(d) => {
                Ok(Expr::function_call(
                    "DATEDIFF",
                    vec![d.end.to_expr()?, d.start.to_expr()?],
                ))
            }
            ExprBlock::Extract(e) => {
                Ok(Expr::function_call(
                    "EXTRACT",
                    vec![Expr::string(&e.part), e.col.to_expr()?],
                ))
            }
            ExprBlock::ToDate(td) => {
                let mut args = vec![td.col.to_expr()?];
                if let Some(fmt) = &td.format {
                    args.push(fmt.to_expr()?);
                }
                Ok(Expr::function_call("TO_DATE", args))
            }
            ExprBlock::ToTimestamp(td) => {
                let mut args = vec![td.col.to_expr()?];
                if let Some(fmt) = &td.format {
                    args.push(fmt.to_expr()?);
                }
                Ok(Expr::function_call("TO_TIMESTAMP", args))
            }

            // ── Type conversion ─────────────────────────────────
            ExprBlock::Cast(c) => {
                let data_type: semstrait_core::DataType = c.to.parse()
                    .map_err(|_| ExprBlockError::InvalidCastType(c.to.clone()))?;
                Ok(Expr::cast(c.col.to_expr()?, data_type))
            }

            // ── Guard ───────────────────────────────────────────
            ExprBlock::Guard(g) => {
                Ok(Expr::guard(g.condition.to_expr()?, g.col.to_expr()?))
            }
        }
    }
}

impl LiteralValue {
    fn to_expr(&self) -> Expr {
        match self {
            LiteralValue::Integer(v) => Expr::int(*v),
            LiteralValue::Float(v) => Expr::float(*v),
            LiteralValue::String(v) => Expr::string(v.clone()),
            LiteralValue::Boolean(v) => Expr::boolean(*v),
            LiteralValue::Null => Expr::null(),
        }
    }
}

// ─── Helper functions ───────────────────────────────────────────────────────

fn two_args(args: &TwoArgs) -> Result<(Expr, Expr), ExprBlockError> {
    match args {
        TwoArgs::Array([l, r]) => Ok((l.to_expr()?, r.to_expr()?)),
        TwoArgs::Map { left, right } => Ok((left.to_expr()?, right.to_expr()?)),
    }
}

fn binary(args: &TwoArgs, op: expr::BinaryOp) -> Result<Expr, ExprBlockError> {
    let (l, r) = two_args(args)?;
    Ok(Expr::binary(l, op, r))
}

fn func1(name: &str, inner: &ExprBlock) -> Result<Expr, ExprBlockError> {
    Ok(Expr::function_call(name, vec![inner.to_expr()?]))
}

fn convert_many(exprs: &[ExprBlock]) -> Result<Vec<Expr>, ExprBlockError> {
    exprs.iter().map(|e| e.to_expr()).collect()
}

// ─── Error type ─────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum ExprBlockError {
    #[error("invalid grain value: '{0}'")]
    InvalidGrain(String),

    #[error("invalid cast type: '{0}'")]
    InvalidCastType(String),
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: parse YAML string -> ExprBlock -> Expr
    fn parse_block(yaml: &str) -> Expr {
        let block: ExprBlock = serde_yaml::from_str(yaml).expect("failed to parse YAML");
        block.to_expr().expect("failed to convert to Expr")
    }

    // ── Leaf nodes ─────────────────────────────────────────────────────

    #[test]
    fn test_bare_string_is_column() {
        let block: ExprBlock = serde_yaml::from_str("amount").unwrap();
        assert_eq!(block.to_expr().unwrap(), Expr::column("amount"));
    }

    #[test]
    fn test_bare_int_is_literal() {
        let block: ExprBlock = serde_yaml::from_str("42").unwrap();
        assert_eq!(block.to_expr().unwrap(), Expr::int(42));
    }

    #[test]
    fn test_bare_float_is_literal() {
        let block: ExprBlock = serde_yaml::from_str("2.5").unwrap();
        assert_eq!(block.to_expr().unwrap(), Expr::float(2.5));
    }

    #[test]
    fn test_bare_bool_is_literal() {
        let block: ExprBlock = serde_yaml::from_str("true").unwrap();
        assert_eq!(block.to_expr().unwrap(), Expr::boolean(true));
    }

    #[test]
    fn test_bare_null_is_literal() {
        let block: ExprBlock = serde_yaml::from_str("null").unwrap();
        assert_eq!(block.to_expr().unwrap(), Expr::null());
    }

    #[test]
    fn test_lit_string() {
        let expr = parse_block("lit: hello");
        assert_eq!(expr, Expr::string("hello"));
    }

    #[test]
    fn test_lit_int() {
        let expr = parse_block("lit: 42");
        assert_eq!(expr, Expr::int(42));
    }

    #[test]
    fn test_lit_float() {
        let expr = parse_block("lit: 2.5");
        assert_eq!(expr, Expr::float(2.5));
    }

    #[test]
    fn test_lit_null() {
        let expr = parse_block("lit: null");
        assert_eq!(expr, Expr::null());
    }

    // ── Arithmetic ──────────────────────────────────────────────────────

    #[test]
    fn test_add() {
        let expr = parse_block("add: [a, b]");
        assert_eq!(expr, Expr::add(Expr::column("a"), Expr::column("b")));
    }

    #[test]
    fn test_subtract() {
        let expr = parse_block("subtract: [revenue, cost]");
        assert_eq!(expr, Expr::subtract(Expr::column("revenue"), Expr::column("cost")));
    }

    #[test]
    fn test_multiply() {
        let expr = parse_block("multiply: [price, qty]");
        assert_eq!(expr, Expr::multiply(Expr::column("price"), Expr::column("qty")));
    }

    #[test]
    fn test_divide() {
        let expr = parse_block("divide: [total, count]");
        assert_eq!(expr, Expr::divide(Expr::column("total"), Expr::column("count")));
    }

    #[test]
    fn test_safe_divide() {
        let expr = parse_block("safe_divide: [revenue, clicks]");
        assert_eq!(
            expr,
            Expr::safe_divide(Expr::column("revenue"), Expr::column("clicks"))
        );
    }

    // ── Comparison ──────────────────────────────────────────────────────

    #[test]
    fn test_eq() {
        let yaml = r#"eq: [status, {lit: "active"}]"#;
        let expr = parse_block(yaml);
        assert_eq!(expr, Expr::eq(Expr::column("status"), Expr::string("active")));
    }

    #[test]
    fn test_not_eq() {
        let yaml = r#"not_eq: [status, {lit: "deleted"}]"#;
        let expr = parse_block(yaml);
        assert_eq!(
            expr,
            Expr::binary(Expr::column("status"), expr::BinaryOp::NotEq, Expr::string("deleted"))
        );
    }

    #[test]
    fn test_lt() {
        let expr = parse_block("lt: [age, 18]");
        assert_eq!(expr, Expr::lt(Expr::column("age"), Expr::int(18)));
    }

    #[test]
    fn test_gt() {
        let expr = parse_block("gt: [score, 100]");
        assert_eq!(expr, Expr::gt(Expr::column("score"), Expr::int(100)));
    }

    #[test]
    fn test_lte() {
        let expr = parse_block("lte: [price, 50]");
        assert_eq!(expr, Expr::lte(Expr::column("price"), Expr::int(50)));
    }

    #[test]
    fn test_gte() {
        let expr = parse_block("gte: [quantity, 1]");
        assert_eq!(expr, Expr::gte(Expr::column("quantity"), Expr::int(1)));
    }

    // ── Logical ─────────────────────────────────────────────────────────

    #[test]
    fn test_and() {
        let expr = parse_block("and: [{gt: [age, 18]}, {lt: [age, 65]}]");
        assert_eq!(
            expr,
            Expr::and(
                Expr::gt(Expr::column("age"), Expr::int(18)),
                Expr::lt(Expr::column("age"), Expr::int(65)),
            )
        );
    }

    #[test]
    fn test_or() {
        let yaml = r#"or: [{eq: [status, {lit: "active"}]}, {eq: [status, {lit: "pending"}]}]"#;
        let expr = parse_block(yaml);
        assert_eq!(
            expr,
            Expr::or(
                Expr::eq(Expr::column("status"), Expr::string("active")),
                Expr::eq(Expr::column("status"), Expr::string("pending")),
            )
        );
    }

    #[test]
    fn test_not() {
        let expr = parse_block("not: {is_null: name}");
        assert_eq!(expr, Expr::not(Expr::is_null(Expr::column("name"))));
    }

    #[test]
    fn test_negate() {
        let expr = parse_block("negate: amount");
        assert_eq!(expr, Expr::negate(Expr::column("amount")));
    }

    // ── Conditional ─────────────────────────────────────────────────────

    #[test]
    fn test_case() {
        let yaml = r#"
case:
  when:
    - condition: {eq: [status, {lit: "active"}]}
      then: 1
    - condition: {eq: [status, {lit: "inactive"}]}
      then: 0
  else: -1
"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::Case(c) => {
                assert_eq!(c.when_then.len(), 2);
                assert!(c.else_expr.is_some());
            }
            _ => panic!("Expected Case"),
        }
    }

    #[test]
    fn test_coalesce() {
        let yaml = r#"coalesce: [preferred, default, {lit: "unknown"}]"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::Coalesce(c) => assert_eq!(c.exprs.len(), 3),
            _ => panic!("Expected Coalesce"),
        }
    }

    #[test]
    fn test_nullif() {
        let expr = parse_block("nullif: [value, 0]");
        assert_eq!(expr, Expr::null_if(Expr::column("value"), Expr::int(0)));
    }

    #[test]
    fn test_nullif_map() {
        let expr = parse_block("nullif: {col: value, null_expr: 0}");
        assert_eq!(expr, Expr::null_if(Expr::column("value"), Expr::int(0)));
    }

    #[test]
    fn test_if_desugars_to_case() {
        let yaml = r#"if: [{eq: [x, 0]}, {lit: "no"}, {lit: "yes"}]"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::Case(c) => {
                assert_eq!(c.when_then.len(), 1);
                assert!(c.else_expr.is_some());
            }
            _ => panic!("Expected Case (from if desugar)"),
        }
    }

    #[test]
    fn test_greatest() {
        let expr = parse_block("greatest: [a, b, c]");
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "GREATEST");
                assert_eq!(fc.args.len(), 3);
            }
            _ => panic!("Expected FunctionCall(GREATEST)"),
        }
    }

    #[test]
    fn test_least() {
        let expr = parse_block("least: [a, b]");
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "LEAST");
                assert_eq!(fc.args.len(), 2);
            }
            _ => panic!("Expected FunctionCall(LEAST)"),
        }
    }

    // ── Predicates ──────────────────────────────────────────────────────

    #[test]
    fn test_in() {
        let yaml = r#"in: [country, {lit: "US"}, {lit: "GB"}, {lit: "DE"}]"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::InList(il) => {
                assert!(!il.negated);
                assert_eq!(il.list.len(), 3);
            }
            _ => panic!("Expected InList"),
        }
    }

    #[test]
    fn test_in_map() {
        let yaml = r#"
in:
  col: country
  list:
    - {lit: "US"}
    - {lit: "GB"}
"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::InList(il) => {
                assert!(!il.negated);
                assert_eq!(il.list.len(), 2);
            }
            _ => panic!("Expected InList"),
        }
    }

    #[test]
    fn test_not_in() {
        let yaml = r#"not_in: [status, {lit: "deleted"}]"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::InList(il) => assert!(il.negated),
            _ => panic!("Expected InList (negated)"),
        }
    }

    #[test]
    fn test_between() {
        let expr = parse_block("between: [age, 18, 65]");
        match &expr {
            Expr::Between(b) => assert!(!b.negated),
            _ => panic!("Expected Between"),
        }
    }

    #[test]
    fn test_between_map() {
        let expr = parse_block("between: {col: age, low: 18, high: 65}");
        match &expr {
            Expr::Between(b) => assert!(!b.negated),
            _ => panic!("Expected Between"),
        }
    }

    #[test]
    fn test_like() {
        let yaml = r#"like: [name, {lit: "%smith%"}]"#;
        let expr = parse_block(yaml);
        assert!(matches!(expr, Expr::Like(_)));
    }

    #[test]
    fn test_ilike() {
        let yaml = r#"ilike: [campaign, {lit: "uk_%"}]"#;
        let expr = parse_block(yaml);
        assert!(matches!(expr, Expr::ILike(_)));
    }

    #[test]
    fn test_is_null() {
        let expr = parse_block("is_null: email");
        assert_eq!(expr, Expr::is_null(Expr::column("email")));
    }

    #[test]
    fn test_is_not_null() {
        let expr = parse_block("is_not_null: phone");
        assert_eq!(expr, Expr::is_not_null(Expr::column("phone")));
    }

    // ── Pattern matching ────────────────────────────────────────────────

    #[test]
    fn test_regexp_match() {
        let yaml = r#"regexp_match: [email, {lit: "@example\\.com"}]"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::RegexpMatch(re) => assert!(!re.full_match),
            _ => panic!("Expected RegexpMatch"),
        }
    }

    #[test]
    fn test_regexp_match_map() {
        let yaml = r#"
regexp_match:
  col: email
  pattern: {lit: "@example\\.com"}
  full_match: true
"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::RegexpMatch(re) => assert!(re.full_match),
            _ => panic!("Expected RegexpMatch"),
        }
    }

    #[test]
    fn test_regexp_extract() {
        let yaml = r#"regexp_extract: [campaign, {lit: "^([A-Z]{2})_"}, 1]"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::RegexpExtract(re) => assert_eq!(re.group_idx, 1),
            _ => panic!("Expected RegexpExtract"),
        }
    }

    #[test]
    fn test_regexp_replace() {
        let yaml = r#"regexp_replace: [text, {lit: "\\d+"}, {lit: "X"}]"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "REGEXP_REPLACE");
                assert_eq!(fc.args.len(), 3);
            }
            _ => panic!("Expected FunctionCall(REGEXP_REPLACE)"),
        }
    }

    #[test]
    fn test_regexp_replace_map() {
        let yaml = r#"
regexp_replace:
  col: text
  pattern: {lit: "\\d+"}
  replacement: {lit: "X"}
"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "REGEXP_REPLACE");
                assert_eq!(fc.args.len(), 3);
            }
            _ => panic!("Expected FunctionCall(REGEXP_REPLACE)"),
        }
    }

    // ── String functions ────────────────────────────────────────────────

    #[test]
    fn test_upper() {
        let expr = parse_block("upper: name");
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "UPPER");
                assert_eq!(fc.args.len(), 1);
                assert_eq!(fc.args[0], Expr::column("name"));
            }
            _ => panic!("Expected FunctionCall(UPPER)"),
        }
    }

    #[test]
    fn test_lower() {
        let expr = parse_block("lower: name");
        match &expr {
            Expr::FunctionCall(fc) => assert_eq!(fc.name, "LOWER"),
            _ => panic!("Expected FunctionCall(LOWER)"),
        }
    }

    #[test]
    fn test_trim() {
        let expr = parse_block("trim: input");
        match &expr {
            Expr::FunctionCall(fc) => assert_eq!(fc.name, "TRIM"),
            _ => panic!("Expected FunctionCall(TRIM)"),
        }
    }

    #[test]
    fn test_ltrim() {
        let expr = parse_block("ltrim: input");
        match &expr {
            Expr::FunctionCall(fc) => assert_eq!(fc.name, "LTRIM"),
            _ => panic!("Expected FunctionCall(LTRIM)"),
        }
    }

    #[test]
    fn test_rtrim() {
        let expr = parse_block("rtrim: input");
        match &expr {
            Expr::FunctionCall(fc) => assert_eq!(fc.name, "RTRIM"),
            _ => panic!("Expected FunctionCall(RTRIM)"),
        }
    }

    #[test]
    fn test_length() {
        let expr = parse_block("length: name");
        match &expr {
            Expr::FunctionCall(fc) => assert_eq!(fc.name, "LENGTH"),
            _ => panic!("Expected FunctionCall(LENGTH)"),
        }
    }

    #[test]
    fn test_concat() {
        let yaml = r#"concat: [first_name, {lit: " "}, last_name]"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "CONCAT");
                assert_eq!(fc.args.len(), 3);
            }
            _ => panic!("Expected FunctionCall(CONCAT)"),
        }
    }

    #[test]
    fn test_replace() {
        let yaml = r#"replace: [url, {lit: "http"}, {lit: "https"}]"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "REPLACE");
                assert_eq!(fc.args.len(), 3);
            }
            _ => panic!("Expected FunctionCall(REPLACE)"),
        }
    }

    #[test]
    fn test_substr() {
        let expr = parse_block("substr: [code, 1, 3]");
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "SUBSTRING");
                assert_eq!(fc.args.len(), 3);
            }
            _ => panic!("Expected FunctionCall(SUBSTRING)"),
        }
    }

    #[test]
    fn test_substr_without_length() {
        let expr = parse_block("substr: [code, 2]");
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "SUBSTRING");
                assert_eq!(fc.args.len(), 2);
            }
            _ => panic!("Expected FunctionCall(SUBSTRING)"),
        }
    }

    #[test]
    fn test_left() {
        let expr = parse_block("left: [name, 5]");
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "LEFT");
                assert_eq!(fc.args.len(), 2);
            }
            _ => panic!("Expected FunctionCall(LEFT)"),
        }
    }

    #[test]
    fn test_right() {
        let expr = parse_block("right: [name, 3]");
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "RIGHT");
                assert_eq!(fc.args.len(), 2);
            }
            _ => panic!("Expected FunctionCall(RIGHT)"),
        }
    }

    #[test]
    fn test_lpad() {
        let yaml = r#"lpad: [id, 10, {lit: "0"}]"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "LPAD");
                assert_eq!(fc.args.len(), 3);
            }
            _ => panic!("Expected FunctionCall(LPAD)"),
        }
    }

    #[test]
    fn test_rpad() {
        let yaml = r#"rpad: [code, 8, {lit: "."}]"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "RPAD");
                assert_eq!(fc.args.len(), 3);
            }
            _ => panic!("Expected FunctionCall(RPAD)"),
        }
    }

    #[test]
    fn test_reverse() {
        let expr = parse_block("reverse: name");
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "REVERSE");
                assert_eq!(fc.args.len(), 1);
            }
            _ => panic!("Expected FunctionCall(REVERSE)"),
        }
    }

    #[test]
    fn test_initcap() {
        let expr = parse_block("initcap: title");
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "INITCAP");
                assert_eq!(fc.args.len(), 1);
            }
            _ => panic!("Expected FunctionCall(INITCAP)"),
        }
    }

    #[test]
    fn test_repeat() {
        let expr = parse_block("repeat: [star, 3]");
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "REPEAT");
                assert_eq!(fc.args.len(), 2);
            }
            _ => panic!("Expected FunctionCall(REPEAT)"),
        }
    }

    #[test]
    fn test_starts_with() {
        let yaml = r#"starts_with: [url, {lit: "https"}]"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "STARTS_WITH");
                assert_eq!(fc.args.len(), 2);
            }
            _ => panic!("Expected FunctionCall(STARTS_WITH)"),
        }
    }

    #[test]
    fn test_ends_with() {
        let yaml = r#"ends_with: [file, {lit: ".csv"}]"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "ENDS_WITH");
                assert_eq!(fc.args.len(), 2);
            }
            _ => panic!("Expected FunctionCall(ENDS_WITH)"),
        }
    }

    #[test]
    fn test_position() {
        let yaml = r#"position: [haystack, {lit: "needle"}]"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "POSITION");
                assert_eq!(fc.args.len(), 2);
            }
            _ => panic!("Expected FunctionCall(POSITION)"),
        }
    }

    #[test]
    fn test_split_part() {
        let yaml = r#"split_part: [name, {lit: "_"}, 1]"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "SPLIT_PART");
                assert_eq!(fc.args.len(), 3);
            }
            _ => panic!("Expected FunctionCall(SPLIT_PART)"),
        }
    }

    #[test]
    fn test_split_part_map() {
        let yaml = r#"split_part: {col: name, delimiter: {lit: "_"}, part: 2}"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "SPLIT_PART");
                assert_eq!(fc.args.len(), 3);
            }
            _ => panic!("Expected FunctionCall(SPLIT_PART)"),
        }
    }

    #[test]
    fn test_concat_ws() {
        let yaml = r#"concat_ws: [{lit: "-"}, a, b, c]"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "CONCAT_WS");
                assert_eq!(fc.args.len(), 4); // separator + 3 values
            }
            _ => panic!("Expected FunctionCall(CONCAT_WS)"),
        }
    }

    #[test]
    fn test_concat_ws_map() {
        let yaml = r#"concat_ws: {separator: {lit: "-"}, exprs: [a, b]}"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "CONCAT_WS");
                assert_eq!(fc.args.len(), 3); // separator + 2 values
            }
            _ => panic!("Expected FunctionCall(CONCAT_WS)"),
        }
    }

    // ── Math functions ──────────────────────────────────────────────────

    #[test]
    fn test_abs() {
        let expr = parse_block("abs: delta");
        match &expr {
            Expr::FunctionCall(fc) => assert_eq!(fc.name, "ABS"),
            _ => panic!("Expected FunctionCall(ABS)"),
        }
    }

    #[test]
    fn test_ceil() {
        let expr = parse_block("ceil: price");
        match &expr {
            Expr::FunctionCall(fc) => assert_eq!(fc.name, "CEIL"),
            _ => panic!("Expected FunctionCall(CEIL)"),
        }
    }

    #[test]
    fn test_floor() {
        let expr = parse_block("floor: price");
        match &expr {
            Expr::FunctionCall(fc) => assert_eq!(fc.name, "FLOOR"),
            _ => panic!("Expected FunctionCall(FLOOR)"),
        }
    }

    #[test]
    fn test_round() {
        let expr = parse_block("round: [price, 2]");
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "ROUND");
                assert_eq!(fc.args.len(), 2);
            }
            _ => panic!("Expected FunctionCall(ROUND)"),
        }
    }

    #[test]
    fn test_power() {
        let expr = parse_block("power: [x, 2]");
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "POWER");
                assert_eq!(fc.args.len(), 2);
            }
            _ => panic!("Expected FunctionCall(POWER)"),
        }
    }

    #[test]
    fn test_sqrt() {
        let expr = parse_block("sqrt: variance");
        match &expr {
            Expr::FunctionCall(fc) => assert_eq!(fc.name, "SQRT"),
            _ => panic!("Expected FunctionCall(SQRT)"),
        }
    }

    #[test]
    fn test_mod() {
        let expr = parse_block("mod: [value, 3]");
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "MOD");
                assert_eq!(fc.args.len(), 2);
            }
            _ => panic!("Expected FunctionCall(MOD)"),
        }
    }

    // ── Date functions ──────────────────────────────────────────────────

    #[test]
    fn test_date_trunc() {
        let yaml = r#"date_trunc: [{lit: "month"}, order_date]"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::DateTrunc(dt) => assert_eq!(dt.grain, Grain::Month),
            _ => panic!("Expected DateTrunc"),
        }
    }

    #[test]
    fn test_date_trunc_map() {
        let yaml = r#"date_trunc: {grain: "month", col: order_date}"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::DateTrunc(dt) => assert_eq!(dt.grain, Grain::Month),
            _ => panic!("Expected DateTrunc"),
        }
    }

    #[test]
    fn test_current_date() {
        let expr = parse_block("current_date: {}");
        match &expr {
            Expr::FunctionCall(fc) => assert_eq!(fc.name, "CURRENT_DATE"),
            _ => panic!("Expected FunctionCall(CURRENT_DATE)"),
        }
    }

    #[test]
    fn test_current_timestamp() {
        let expr = parse_block("current_timestamp: {}");
        match &expr {
            Expr::FunctionCall(fc) => assert_eq!(fc.name, "CURRENT_TIMESTAMP"),
            _ => panic!("Expected FunctionCall(CURRENT_TIMESTAMP)"),
        }
    }

    #[test]
    fn test_date_add() {
        let expr = parse_block("date_add: [order_date, 30]");
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "DATE_ADD");
                assert_eq!(fc.args.len(), 2);
            }
            _ => panic!("Expected FunctionCall(DATE_ADD)"),
        }
    }

    #[test]
    fn test_date_diff() {
        let expr = parse_block("date_diff: [start_date, end_date]");
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "DATEDIFF");
                assert_eq!(fc.args.len(), 2);
            }
            _ => panic!("Expected FunctionCall(DATEDIFF)"),
        }
    }

    #[test]
    fn test_extract() {
        let yaml = r#"extract: [{lit: "year"}, order_date]"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "EXTRACT");
                assert_eq!(fc.args.len(), 2);
            }
            _ => panic!("Expected FunctionCall(EXTRACT)"),
        }
    }

    #[test]
    fn test_to_date() {
        let expr = parse_block("to_date: [str_col]");
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "TO_DATE");
                assert_eq!(fc.args.len(), 1);
            }
            _ => panic!("Expected FunctionCall(TO_DATE)"),
        }
    }

    #[test]
    fn test_to_date_with_format() {
        let yaml = r#"to_date: [str_col, {lit: "%Y-%m-%d"}]"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "TO_DATE");
                assert_eq!(fc.args.len(), 2);
            }
            _ => panic!("Expected FunctionCall(TO_DATE)"),
        }
    }

    #[test]
    fn test_to_date_map() {
        let yaml = r#"to_date: {col: str_col, format: {lit: "%Y-%m-%d"}}"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "TO_DATE");
                assert_eq!(fc.args.len(), 2);
            }
            _ => panic!("Expected FunctionCall(TO_DATE)"),
        }
    }

    #[test]
    fn test_to_timestamp() {
        let expr = parse_block("to_timestamp: [str_col]");
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "TO_TIMESTAMP");
                assert_eq!(fc.args.len(), 1);
            }
            _ => panic!("Expected FunctionCall(TO_TIMESTAMP)"),
        }
    }

    #[test]
    fn test_to_timestamp_with_format() {
        let yaml = r#"to_timestamp: [str_col, {lit: "%Y-%m-%dT%H:%M:%S"}]"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::FunctionCall(fc) => {
                assert_eq!(fc.name, "TO_TIMESTAMP");
                assert_eq!(fc.args.len(), 2);
            }
            _ => panic!("Expected FunctionCall(TO_TIMESTAMP)"),
        }
    }

    // ── Type conversion ─────────────────────────────────────────────────

    #[test]
    fn test_cast() {
        let yaml = r#"cast: [price, {lit: "DECIMAL(10,2)"}]"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::Cast(c) => {
                assert_eq!(*c.expr, Expr::column("price"));
                assert_eq!(c.data_type, semstrait_core::DataType::Decimal { precision: 10, scale: 2 });
            }
            _ => panic!("Expected Cast"),
        }
    }

    #[test]
    fn test_cast_map() {
        let yaml = r#"cast: {col: price, to: "VARCHAR"}"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::Cast(c) => {
                assert_eq!(*c.expr, Expr::column("price"));
                assert_eq!(c.data_type, semstrait_core::DataType::String);
            }
            _ => panic!("Expected Cast"),
        }
    }

    // ── Guard ───────────────────────────────────────────────────────────

    #[test]
    fn test_guard() {
        let yaml = r#"guard: [{eq: [cat, {lit: "electronics"}]}, amount]"#;
        let expr = parse_block(yaml);
        assert!(matches!(expr, Expr::Guard(_)));
    }

    #[test]
    fn test_guard_map() {
        let yaml = r#"guard: {condition: {eq: [cat, {lit: "electronics"}]}, col: amount}"#;
        let expr = parse_block(yaml);
        assert!(matches!(expr, Expr::Guard(_)));
    }

    // ── Negative tests ──────────────────────────────────────────────────

    #[test]
    fn test_empty_map_rejected() {
        let result = serde_yaml::from_str::<ExprBlock>("{}");
        assert!(result.is_err(), "empty map should be rejected");
    }

    #[test]
    fn test_multi_key_map_rejected() {
        let yaml = "{lit: 1, add: [a, b]}";
        let result = serde_yaml::from_str::<ExprBlock>(yaml);
        assert!(result.is_err(), "multi-key map should be rejected");
    }

    #[test]
    fn test_unknown_key_rejected() {
        let yaml = "{foobar: x}";
        let result = serde_yaml::from_str::<ExprBlock>(yaml);
        assert!(result.is_err(), "unknown key should be rejected");
    }

    #[test]
    fn test_old_tags_rejected() {
        // Old tags (column:, literal:, in_list:, null_if:, substring:) are removed
        assert!(serde_yaml::from_str::<ExprBlock>("column: amount").is_err());
        assert!(serde_yaml::from_str::<ExprBlock>("literal: 42").is_err());
        assert!(serde_yaml::from_str::<ExprBlock>("in_list: {col: x, list: [1]}").is_err());
        assert!(serde_yaml::from_str::<ExprBlock>("not_in_list: {col: x, list: [1]}").is_err());
        assert!(serde_yaml::from_str::<ExprBlock>("null_if: {col: x, null_expr: 0}").is_err());
        assert!(serde_yaml::from_str::<ExprBlock>("substring: {col: x, start: 1}").is_err());
    }

    // ── Serialize round-trip ────────────────────────────────────────────

    #[test]
    fn test_serialize_round_trip_simple() {
        let block = ExprBlock::Upper(Box::new(ExprBlock::Column("name".to_string())));
        let yaml = serde_yaml::to_string(&block).unwrap();
        let reparsed: ExprBlock = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(block.to_expr().unwrap(), reparsed.to_expr().unwrap());
    }

    #[test]
    fn test_serialize_round_trip_nested() {
        let block = ExprBlock::Add(TwoArgs::Array([
            Box::new(ExprBlock::Column("a".to_string())),
            Box::new(ExprBlock::Multiply(TwoArgs::Array([
                Box::new(ExprBlock::Column("b".to_string())),
                Box::new(ExprBlock::Literal(LiteralValue::Integer(2))),
            ]))),
        ]));
        let yaml = serde_yaml::to_string(&block).unwrap();
        let reparsed: ExprBlock = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(block.to_expr().unwrap(), reparsed.to_expr().unwrap());
    }

    // ── Map form for binary ops ─────────────────────────────────────────

    #[test]
    fn test_binary_map_form() {
        let yaml = "subtract: {left: a, right: b}";
        let expr = parse_block(yaml);
        assert_eq!(expr, Expr::subtract(Expr::column("a"), Expr::column("b")));
    }

    // ── ExprSource ──────────────────────────────────────────────────────

    #[test]
    fn test_expr_source_inline() {
        let yaml = "\"cost / clicks\"";
        let source: ExprSource = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(source, ExprSource::Inline(_)));
    }

    #[test]
    fn test_expr_source_declarative() {
        let yaml = "upper: name";
        let source: ExprSource = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(source, ExprSource::Declarative(_)));
    }

    // ── Complex nested ──────────────────────────────────────────────────

    #[test]
    fn test_nested_complex() {
        let yaml = r#"
case:
  when:
    - condition:
        in: [source, {lit: "google"}, {lit: "facebook"}]
      then:
        regexp_extract: [campaign, {lit: "^([A-Z]{2})_"}, 1]
  else:
    lit: ""
"#;
        let expr = parse_block(yaml);
        match &expr {
            Expr::Case(c) => {
                assert_eq!(c.when_then.len(), 1);
                match &c.when_then[0].result {
                    Expr::RegexpExtract(re) => assert_eq!(re.group_idx, 1),
                    other => panic!("Expected RegexpExtract in then, got {:?}", other),
                }
            }
            _ => panic!("Expected Case"),
        }
    }
}
