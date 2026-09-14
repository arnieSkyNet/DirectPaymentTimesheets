//! Read-only compatibility for existing historical payroll adjustments.
//! The one-off writer is retired; this persisted reason still selects legacy
//! calculation and display semantics and must remain an exact match.

const HISTORICAL_ADJUSTMENT_REASON: &str = "Verified historical payroll backfill (2026-09-04)";

pub(crate) fn is_historical_adjustment_reason(reason: Option<&str>) -> bool {
    reason == Some(HISTORICAL_ADJUSTMENT_REASON)
}

#[cfg(test)]
mod tests {
    use super::is_historical_adjustment_reason;

    #[test]
    fn recognises_the_persisted_legacy_reason_exactly() {
        assert!(is_historical_adjustment_reason(Some(
            "Verified historical payroll backfill (2026-09-04)"
        )));
    }

    #[test]
    fn does_not_reinterpret_other_adjustment_reasons() {
        for reason in [
            None,
            Some(""),
            Some("Manual correction"),
            Some("Verified historical payroll backfill"),
            Some("Verified historical payroll backfill (2026-09-05)"),
            Some("verified historical payroll backfill (2026-09-04)"),
            Some(" Verified historical payroll backfill (2026-09-04)"),
            Some("Verified historical payroll backfill (2026-09-04) "),
        ] {
            assert!(!is_historical_adjustment_reason(reason), "{reason:?}");
        }
    }
}
