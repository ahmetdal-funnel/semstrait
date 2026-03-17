//! Common types used across semstrait crates

use std::fmt;

/// A glob pattern for matching table names during catalog operations.
///
/// Used by `CatalogProvider::list_tables` for pattern-based table discovery.
///
/// # Examples
///
/// ```
/// use semstrait_core::GlobPattern;
///
/// let pattern = GlobPattern::new("sales_*");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GlobPattern(pub String);

impl GlobPattern {
    /// Creates a new glob pattern from a string.
    pub fn new(pattern: impl Into<String>) -> Self {
        Self(pattern.into())
    }

    /// Returns the pattern as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Tests if `name` matches this glob pattern.
    ///
    /// Supports `*` (any chars) and `?` (single char).
    pub fn matches(&self, name: &str) -> bool {
        glob_match(&self.0, name)
    }
}

impl fmt::Display for GlobPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for GlobPattern {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for GlobPattern {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Simple glob matching: `*` matches any sequence, `?` matches single char.
fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (pn, tn) = (p.len(), t.len());
    // dp[i][j] = pattern[0..i] matches text[0..j]
    let mut dp = vec![vec![false; tn + 1]; pn + 1];
    dp[0][0] = true;
    for i in 1..=pn {
        if p[i - 1] == '*' {
            dp[i][0] = dp[i - 1][0];
        }
    }
    for i in 1..=pn {
        for j in 1..=tn {
            if p[i - 1] == '*' {
                dp[i][j] = dp[i - 1][j] || dp[i][j - 1];
            } else if p[i - 1] == '?' || p[i - 1] == t[j - 1] {
                dp[i][j] = dp[i - 1][j - 1];
            }
        }
    }
    dp[pn][tn]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_pattern_creation() {
        let pattern = GlobPattern::new("test_*");
        assert_eq!(pattern.as_str(), "test_*");
    }

    #[test]
    fn test_glob_pattern_from_string() {
        let pattern: GlobPattern = "sales_*".into();
        assert_eq!(pattern.as_str(), "sales_*");
    }

    #[test]
    fn test_glob_pattern_display() {
        let pattern = GlobPattern::new("users_*");
        assert_eq!(format!("{}", pattern), "users_*");
    }

    #[test]
    fn test_glob_matches() {
        let p = GlobPattern::new("*");
        assert!(p.matches("anything"));
        assert!(p.matches(""));

        let p = GlobPattern::new("orders_*");
        assert!(p.matches("orders_daily"));
        assert!(p.matches("orders_"));
        assert!(!p.matches("orders"));
        assert!(!p.matches("customers_daily"));

        let p = GlobPattern::new("table_?");
        assert!(p.matches("table_a"));
        assert!(!p.matches("table_ab"));
        assert!(!p.matches("table_"));

        let p = GlobPattern::new("exact");
        assert!(p.matches("exact"));
        assert!(!p.matches("exactlyno"));
    }
}
