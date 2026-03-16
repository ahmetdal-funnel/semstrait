//! Temporal historization filter injection.
//!
//! Generates WHERE clause predicates based on a dataset's temporal
//! historization configuration (timeseries, snapshot, SCD types).

use crate::planner::ir::expr::{BinaryOperator, Column, Expr};
use crate::schema::model::{ScdType, TemporalConfig, TemporalHistorization};

/// Result of temporal filter injection.
#[derive(Debug, Clone)]
pub enum TemporalFilter {
    /// No filter needed for this temporal type.
    None,
    /// A predicate to add to the WHERE clause.
    Predicate(Expr),
}

/// Generate the temporal filter for a dataset's configuration.
///
/// Rules per temporal type:
/// - Timeseries: no implicit filter (user provides time range).
/// - Snapshot: `col = (SELECT MAX(col) FROM same_table)` — approximated as
///   a placeholder predicate. The planner must materialize the sub-query.
/// - SCD Type 1: no filter (destructive updates, always current).
/// - SCD Type 2/5/6: `valid_to IS NULL` (current row).
/// - SCD Type 3/4: no row-level filter.
pub fn temporal_filter(
    temporal: &TemporalConfig,
    table_alias: &str,
) -> TemporalFilter {
    match &temporal.temporal_type {
        TemporalHistorization::Timeseries(_) => TemporalFilter::None,
        TemporalHistorization::Snapshot(snap) => {
            // Emit a placeholder: snapshot_col = __LATEST_SNAPSHOT__
            // The compiler must replace this with a correlated sub-query.
            let col = Column::new(table_alias, &snap.snapshotted_at);
            let pred = Expr::BinaryOp {
                left: Box::new(Expr::Column(col)),
                op: BinaryOperator::Eq,
                right: Box::new(Expr::Sql(format!(
                    "(SELECT MAX({}) FROM {})",
                    snap.snapshotted_at, table_alias
                ))),
            };
            TemporalFilter::Predicate(pred)
        }
        TemporalHistorization::Scd(scd) => match &scd.scd_type {
            ScdType::Type1 | ScdType::Type3 | ScdType::Type4 => TemporalFilter::None,
            ScdType::Type2(cols) | ScdType::Type5(cols) | ScdType::Type6(cols) => {
                let pred = Expr::IsNull(Box::new(Expr::Column(Column::new(
                    table_alias,
                    &cols.valid_to,
                ))));
                TemporalFilter::Predicate(pred)
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::model::*;

    fn config(hist: TemporalHistorization) -> TemporalConfig {
        TemporalConfig {
            temporal_type: hist,
        }
    }

    #[test]
    fn test_timeseries_no_filter() {
        let tc = config(TemporalHistorization::Timeseries(TimeseriesConfig {
            occurred_at: "event_time".into(),
        }));
        let result = temporal_filter(&tc, "events");
        assert!(matches!(result, TemporalFilter::None));
    }

    #[test]
    fn test_snapshot_generates_predicate() {
        let tc = config(TemporalHistorization::Snapshot(SnapshotConfig {
            snapshotted_at: "snap_date".into(),
        }));
        let result = temporal_filter(&tc, "accounts");
        match result {
            TemporalFilter::Predicate(expr) => {
                let s = expr.to_string();
                assert!(s.contains("snap_date"));
                assert!(s.contains("MAX"));
            }
            TemporalFilter::None => panic!("expected predicate for snapshot"),
        }
    }

    #[test]
    fn test_scd_type1_no_filter() {
        let tc = config(TemporalHistorization::Scd(ScdConfig {
            scd_type: ScdType::Type1,
        }));
        let result = temporal_filter(&tc, "t");
        assert!(matches!(result, TemporalFilter::None));
    }

    #[test]
    fn test_scd_type2_valid_to_is_null() {
        let tc = config(TemporalHistorization::Scd(ScdConfig {
            scd_type: ScdType::Type2(ScdVersionedColumns {
                valid_from: "eff_date".into(),
                valid_to: "exp_date".into(),
            }),
        }));
        let result = temporal_filter(&tc, "accounts");
        match result {
            TemporalFilter::Predicate(expr) => {
                let s = expr.to_string();
                assert!(s.contains("exp_date"));
                assert!(s.contains("IS NULL"));
            }
            TemporalFilter::None => panic!("expected predicate for SCD type 2"),
        }
    }

    #[test]
    fn test_scd_type3_no_filter() {
        let tc = config(TemporalHistorization::Scd(ScdConfig {
            scd_type: ScdType::Type3,
        }));
        let result = temporal_filter(&tc, "t");
        assert!(matches!(result, TemporalFilter::None));
    }

    #[test]
    fn test_scd_type5_valid_to_is_null() {
        let tc = config(TemporalHistorization::Scd(ScdConfig {
            scd_type: ScdType::Type5(ScdVersionedColumns {
                valid_from: "start".into(),
                valid_to: "end".into(),
            }),
        }));
        let result = temporal_filter(&tc, "t");
        match result {
            TemporalFilter::Predicate(expr) => {
                let s = expr.to_string();
                assert!(s.contains("end"));
                assert!(s.contains("IS NULL"));
            }
            TemporalFilter::None => panic!("expected predicate"),
        }
    }
}
