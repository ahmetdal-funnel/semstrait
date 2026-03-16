//! Domain pre-filtering for candidate selection.
//!
//! Before constraint checking, filters the candidate set (datasets/kinds) by
//! domain prefix matching. Candidates with no domain are always included.

use crate::schema::model::DomainSpec;

/// Check whether a candidate's domain matches the requested domain.
///
/// Rules:
/// - If the candidate has no domain, it always matches (universal).
/// - If no domain is requested, all candidates match.
/// - Otherwise, at least one of the candidate's domains must match the
///   requested domain via prefix match (dot-delimited hierarchy).
///   E.g. request "financial" matches candidate "financial.transactions".
pub fn domain_matches(candidate_domain: Option<&DomainSpec>, requested: Option<&str>) -> bool {
    let requested = match requested {
        Some(r) => r,
        None => return true,
    };

    let spec = match candidate_domain {
        Some(s) => s,
        None => return true, // no domain = universal
    };

    spec.0.iter().any(|d| is_domain_prefix(requested, d))
}

/// Check if `prefix` is a domain prefix of `domain`.
///
/// "financial" is a prefix of "financial.transactions" and "financial".
/// "financial.t" is NOT a prefix of "financial.transactions" (must align on dots).
fn is_domain_prefix(prefix: &str, domain: &str) -> bool {
    if domain.starts_with(prefix) {
        // Must match exactly or be followed by a dot
        domain.len() == prefix.len() || domain.as_bytes().get(prefix.len()) == Some(&b'.')
    } else {
        // Also check the reverse: candidate domain is a prefix of requested
        prefix.starts_with(domain)
            && (prefix.len() == domain.len() || prefix.as_bytes().get(domain.len()) == Some(&b'.'))
    }
}

/// Filter a slice of items by domain, returning indices of matching items.
#[allow(dead_code)]
pub fn filter_by_domain<F>(
    items: &[F],
    get_domain: impl Fn(&F) -> Option<&DomainSpec>,
    requested: Option<&str>,
) -> Vec<usize> {
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| domain_matches(get_domain(item), requested))
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(domains: &[&str]) -> DomainSpec {
        DomainSpec(domains.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn test_no_requested_domain_matches_all() {
        assert!(domain_matches(Some(&spec(&["financial"])), None));
        assert!(domain_matches(None, None));
    }

    #[test]
    fn test_no_candidate_domain_matches_always() {
        assert!(domain_matches(None, Some("financial")));
    }

    #[test]
    fn test_exact_match() {
        assert!(domain_matches(
            Some(&spec(&["financial.transactions"])),
            Some("financial.transactions")
        ));
    }

    #[test]
    fn test_prefix_match_requested_broader() {
        assert!(domain_matches(
            Some(&spec(&["financial.transactions"])),
            Some("financial")
        ));
    }

    #[test]
    fn test_prefix_match_candidate_broader() {
        assert!(domain_matches(
            Some(&spec(&["financial"])),
            Some("financial.transactions")
        ));
    }

    #[test]
    fn test_no_match() {
        assert!(!domain_matches(
            Some(&spec(&["marketing.ads"])),
            Some("financial")
        ));
    }

    #[test]
    fn test_partial_segment_no_match() {
        // "financial.t" should NOT match "financial.transactions"
        assert!(!domain_matches(
            Some(&spec(&["financial.transactions"])),
            Some("financial.t")
        ));
    }

    #[test]
    fn test_multi_domain_one_matches() {
        assert!(domain_matches(
            Some(&spec(&["marketing.ads", "financial.transactions"])),
            Some("financial")
        ));
    }

    #[test]
    fn test_filter_by_domain() {
        let items = vec![
            Some(spec(&["financial.orders"])),
            None,
            Some(spec(&["marketing.ads"])),
            Some(spec(&["financial.payments"])),
        ];
        let indices = filter_by_domain(&items, |i| i.as_ref(), Some("financial"));
        assert_eq!(indices, vec![0, 1, 3]); // index 1 has no domain → included
    }
}
