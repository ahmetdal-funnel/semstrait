//! `${VAR}` substitution — `32 §8`, `32b §6`.

use crate::error::parse::ParseErrorKind;
use crate::error::catalogs::CatalogsParseErrorKind;
use semstrait_core::diagnostic::Diagnostic;

/// Substitute `${IDENT}` tokens against `std::env::var`. Bare `$VAR`
/// is left as literal text. Unset variables raise the supplied
/// stage-kind via the converter `to_kind`.
pub(crate) fn substitute_env<F, K>(input: &str, to_kind: F) -> Result<String, Diagnostic<K>>
where
    F: Fn(&str) -> K,
    K: semstrait_core::diagnostic::Diagnose,
{
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            let mut name = String::new();
            let mut closed = false;
            for c in chars.by_ref() {
                if c == '}' {
                    closed = true;
                    break;
                }
                name.push(c);
            }
            if !closed {
                return Err(Diagnostic::new(to_kind("<unterminated>")));
            }
            if name.is_empty() {
                return Err(Diagnostic::new(to_kind("<empty>")));
            }
            match std::env::var(&name) {
                Ok(value) => out.push_str(&value),
                Err(_) => return Err(Diagnostic::new(to_kind(&name))),
            }
        } else {
            out.push(ch);
        }
    }
    Ok(out)
}

/// Convenience wrapper for `parse` (model side). Returns
/// [`ParseErrorKind::UnsetEnvVar`] on missing-or-malformed envs.
pub(crate) fn substitute_env_for_model(input: &str) -> Result<String, Diagnostic<ParseErrorKind>> {
    substitute_env(input, |var| ParseErrorKind::UnsetEnvVar {
        var: var.to_string(),
    })
}

/// Convenience wrapper for `parse_catalogs`.
pub(crate) fn substitute_env_for_catalogs(
    input: &str,
) -> Result<String, Diagnostic<CatalogsParseErrorKind>> {
    substitute_env(input, |var| CatalogsParseErrorKind::UnsetEnvVar {
        var: var.to_string(),
    })
}
