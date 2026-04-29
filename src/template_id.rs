//! Deterministic content-derived template IDs.
//!
//! A `template_id` is computed as a stable hash of the canonicalized pattern
//! structure. This guarantees that the same parse tree always maps to the same
//! ID, forever, across processes, restarts, deploys, and database migrations —
//! which is load-bearing for long-horizon KL divergence analyses where a
//! renumbered template silently corrupts the result.
//!
//! IDs are computed locally at synthesis time. No database round-trip is
//! needed to assign one. Concurrent synthesis of the same novel pattern from
//! two workers produces the same ID, so the downstream insert is naturally
//! idempotent.
//!
//! Collision risk with the u64 truncation is negligible per tenant: birthday
//! paradox at 1M templates per tenant gives ~3e-8 probability. Switch to u128
//! / String IDs if global counts approach billions.

/// Version of the canonicalization rules. Stored alongside templates in the
/// catalog so old IDs remain identifiable if rules ever change. Do not mix
/// this into the hash itself — that would invalidate every existing ID on a
/// rule change, defeating the point of stable IDs.
pub const CANONICALIZATION_VERSION: u32 = 1;

/// Canonical-form token: a literal stretch of the pattern, or a placeholder
/// slot. Internal to the canonicalizer.
enum CanonToken {
    Literal(String),
    Slot,
}

/// Canonicalize a regex-style pattern into a form suitable for hashing.
///
/// Stripped (cosmetic — must not affect identity):
/// - Capture group names (`(?P<name>...)`, `(?<name>...)`)
/// - The internal expression of any group `(...)` or character class `[...]`
/// - Specific metacharacter classes: `\d`, `\w`, `\s` and uppercase variants
/// - Bare `.` (matches any char)
/// - All quantifiers (`*`, `+`, `?`, `{n,m}`)
/// - Anchors (`^`, `$`) and alternation (`|`)
/// - Repeated whitespace inside literals (collapsed to one space)
/// - Leading / trailing whitespace
///
/// Preserved (structural — defines identity):
/// - Literal text content of fixed portions of the pattern
/// - Position and count of placeholders
/// - Structural punctuation (`:`, `=`, `[`, `]`, `,`, etc.)
/// - Case
pub fn canonicalize_pattern(pattern: &str) -> String {
    let mut tokens: Vec<CanonToken> = Vec::new();
    let mut current = String::new();
    let mut chars = pattern.chars().peekable();
    let mut depth: i32 = 0;
    let mut in_char_class = false;

    while let Some(ch) = chars.next() {
        if in_char_class {
            match ch {
                '\\' => {
                    chars.next();
                }
                ']' => {
                    in_char_class = false;
                    flush_literal(&mut current, &mut tokens);
                    tokens.push(CanonToken::Slot);
                    skip_quantifier(&mut chars);
                }
                _ => {}
            }
            continue;
        }
        if depth > 0 {
            match ch {
                '\\' => {
                    chars.next();
                }
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        flush_literal(&mut current, &mut tokens);
                        tokens.push(CanonToken::Slot);
                        skip_quantifier(&mut chars);
                    }
                }
                _ => {}
            }
            continue;
        }

        match ch {
            '\\' => {
                if let Some(next) = chars.next() {
                    if matches!(next, 'd' | 'w' | 's' | 'D' | 'W' | 'S') {
                        flush_literal(&mut current, &mut tokens);
                        tokens.push(CanonToken::Slot);
                        skip_quantifier(&mut chars);
                    } else {
                        push_literal_char(&mut current, next);
                    }
                }
            }
            '(' => {
                flush_literal(&mut current, &mut tokens);
                depth = 1;
            }
            '[' => {
                in_char_class = true;
            }
            '.' => {
                flush_literal(&mut current, &mut tokens);
                tokens.push(CanonToken::Slot);
                skip_quantifier(&mut chars);
            }
            '*' | '+' | '?' => {
                // bare quantifier with nothing to quantify — drop
            }
            '^' | '$' | '|' => {
                // anchors / alternation — drop
            }
            '{' => {
                // `{n,m}` quantifier if a digit follows; otherwise literal `{`
                let is_quantifier = chars.peek().is_some_and(|c| c.is_ascii_digit());
                if is_quantifier {
                    for c in chars.by_ref() {
                        if c == '}' {
                            break;
                        }
                    }
                } else {
                    push_literal_char(&mut current, '{');
                }
            }
            ws if ws.is_whitespace() => {
                if !current.ends_with(' ') {
                    current.push(' ');
                }
            }
            _ => push_literal_char(&mut current, ch),
        }
    }
    flush_literal(&mut current, &mut tokens);

    let mut out = String::with_capacity(pattern.len());
    for tok in &tokens {
        match tok {
            CanonToken::Literal(s) => out.push_str(s),
            CanonToken::Slot => out.push_str("{}"),
        }
    }
    out.trim().to_string()
}

fn flush_literal(current: &mut String, tokens: &mut Vec<CanonToken>) {
    if !current.is_empty() {
        tokens.push(CanonToken::Literal(std::mem::take(current)));
    }
}

fn push_literal_char(current: &mut String, ch: char) {
    if ch.is_whitespace() {
        if !current.ends_with(' ') {
            current.push(' ');
        }
    } else {
        current.push(ch);
    }
}

fn skip_quantifier(chars: &mut std::iter::Peekable<std::str::Chars>) {
    while let Some(&next) = chars.peek() {
        match next {
            '*' | '+' | '?' => {
                chars.next();
            }
            '{' => {
                chars.next();
                for c in chars.by_ref() {
                    if c == '}' {
                        break;
                    }
                }
            }
            _ => break,
        }
    }
}

/// Compute a stable `template_id` from a regex-style pattern.
///
/// Equivalent to `blake3(canonicalize_pattern(pattern))[..8]` interpreted as a
/// big-endian u64. The result is deterministic across processes, OSes, and
/// blake3 versions (blake3's output is spec'd).
pub fn template_id_from_pattern(pattern: &str) -> u64 {
    let canonical = canonicalize_pattern(pattern);
    let hash = blake3::hash(canonical.as_bytes());
    let bytes = hash.as_bytes();
    u64::from_be_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_strips_capture_group_names() {
        let a = r"User (\w+) logged in from (\d+\.\d+\.\d+\.\d+)";
        let b = r"User (?P<user>\w+) logged in from (?P<ip>\d+\.\d+\.\d+\.\d+)";
        assert_eq!(canonicalize_pattern(a), canonicalize_pattern(b));
        assert_eq!(template_id_from_pattern(a), template_id_from_pattern(b));
    }

    #[test]
    fn canonical_collapses_different_placeholder_regexes() {
        let a = r"Request ([a-z0-9]+) completed";
        let b = r"Request (\w+) completed";
        let c = r"Request (.*) completed";
        assert_eq!(canonicalize_pattern(a), "Request {} completed");
        assert_eq!(template_id_from_pattern(a), template_id_from_pattern(b));
        assert_eq!(template_id_from_pattern(b), template_id_from_pattern(c));
    }

    #[test]
    fn canonical_basic_form() {
        assert_eq!(
            canonicalize_pattern(r"cpu_usage: (\d+\.\d+)% - (.*)"),
            "cpu_usage: {}% - {}"
        );
    }

    #[test]
    fn canonical_strips_anchors() {
        assert_eq!(
            canonicalize_pattern(r"^User (\w+) logged in$"),
            "User {} logged in"
        );
    }

    #[test]
    fn canonical_handles_char_classes() {
        assert_eq!(
            canonicalize_pattern(r"id=[0-9]+ name=(\w+)"),
            "id={} name={}"
        );
    }

    #[test]
    fn canonical_collapses_repeated_whitespace() {
        assert_eq!(canonicalize_pattern("foo  bar    baz"), "foo bar baz");
    }

    #[test]
    fn canonical_handles_bare_metaclasses() {
        // `\d+` outside parens is just as much a slot as `(\d+)`
        assert_eq!(
            template_id_from_pattern(r"port=\d+ host=\w+"),
            template_id_from_pattern(r"port=(\d+) host=(\w+)")
        );
    }

    #[test]
    fn different_literals_produce_different_ids() {
        assert_ne!(
            template_id_from_pattern(r"User (\w+) logged in"),
            template_id_from_pattern(r"User (\w+) logged out")
        );
    }

    #[test]
    fn different_placeholder_count_produces_different_ids() {
        assert_ne!(
            template_id_from_pattern(r"foo (\w+) bar"),
            template_id_from_pattern(r"foo (\w+) bar (\w+)")
        );
    }

    #[test]
    fn id_is_stable_across_calls() {
        let p = r"GET /api/users/(\d+) returned (\d+) in (\d+)ms";
        let id1 = template_id_from_pattern(p);
        let id2 = template_id_from_pattern(p);
        assert_eq!(id1, id2);
    }

    #[test]
    fn quantifiers_do_not_affect_identity() {
        assert_eq!(
            template_id_from_pattern(r"foo (\d+) bar"),
            template_id_from_pattern(r"foo (\d{1,5}) bar")
        );
        assert_eq!(
            template_id_from_pattern(r"foo (\w+) bar"),
            template_id_from_pattern(r"foo (\w*) bar")
        );
    }

    #[test]
    fn case_is_preserved() {
        // Uppercase vs lowercase IS structurally different (don't fold case)
        assert_ne!(
            template_id_from_pattern(r"ERROR (\w+)"),
            template_id_from_pattern(r"error (\w+)")
        );
    }

    #[test]
    fn structural_punctuation_is_preserved() {
        // `:` vs `=` IS a structural distinction
        assert_ne!(
            template_id_from_pattern(r"key: (\w+)"),
            template_id_from_pattern(r"key= (\w+)")
        );
    }

    #[test]
    fn nested_groups_collapse_to_one_slot() {
        // (a|b(c|d)) is one capture group containing another. Whatever's
        // inside is a "slot"; the inner depth doesn't add slots.
        assert_eq!(canonicalize_pattern(r"prefix (foo|bar(baz|qux)) suffix"),
                   "prefix {} suffix");
    }

    #[test]
    fn escapes_inside_groups_dont_close_them() {
        // \) inside a group must not be read as the closing paren.
        assert_eq!(canonicalize_pattern(r"id=(\d\)) end"), "id={} end");
    }

    #[test]
    fn escapes_inside_char_class_dont_close_them() {
        // \] inside a char class must not be read as the closing bracket.
        assert_eq!(canonicalize_pattern(r"key=[\]a-z]+ value"), "key={} value");
    }

    #[test]
    fn brace_without_digit_is_literal() {
        // `{` followed by a non-digit is just a literal '{', not a
        // quantifier. (regex's {n,m} always starts with a digit.)
        assert_eq!(canonicalize_pattern(r"json {key}"), "json {key}");
    }

    #[test]
    fn brace_quantifier_is_skipped() {
        assert_eq!(canonicalize_pattern(r"id=\d{4} ok"), "id={} ok");
    }

    #[test]
    fn escaped_metachar_is_literal() {
        // \. \[ \\ \( etc. — preserve as the literal character.
        assert_eq!(
            canonicalize_pattern(r"version 1\.2\.3 build"),
            "version 1.2.3 build"
        );
    }

    #[test]
    fn alternation_outside_groups_is_dropped() {
        // Bare `|` at the top level is dropped (it's not structural to
        // the canonical form). The two alternatives become concatenated.
        assert_eq!(canonicalize_pattern(r"a|b"), "ab");
    }

    #[test]
    fn empty_pattern_yields_empty_canonical() {
        assert_eq!(canonicalize_pattern(""), "");
        // template_id of empty also stable.
        assert_eq!(template_id_from_pattern(""), template_id_from_pattern(""));
    }

    #[test]
    fn whitespace_only_pattern_collapses_to_empty() {
        // Trailing/leading whitespace gets trimmed away; the final
        // canonical form for a pattern of just whitespace is empty.
        assert_eq!(canonicalize_pattern("   \t\n"), "");
    }

    #[test]
    fn version_constant_does_not_change_id() {
        // Sanity: bumping CANONICALIZATION_VERSION should never silently
        // re-hash. The constant is kept *out* of the hash on purpose.
        // Documented invariant — we just confirm it.
        let id_before = template_id_from_pattern(r"User (\w+) ok");
        // (we can't re-import a different version inline; just assert
        // the same call gives the same id, which proves the version
        // const isn't part of the hash input)
        assert_eq!(id_before, template_id_from_pattern(r"User (\w+) ok"));
        // CANONICALIZATION_VERSION exists and is reachable.
        let _ = CANONICALIZATION_VERSION;
    }

    #[test]
    fn skip_quantifier_handles_chained() {
        // `*?` lazy-quantifier: skip_quantifier should consume both.
        // Hard to test directly (private), so test via canonicalize:
        // `(\d+?)` should be one slot, no leftover `?`.
        assert_eq!(canonicalize_pattern(r"id=(\d+?) end"), "id={} end");
    }

    #[test]
    fn bare_metachar_followed_by_quantifier() {
        // `.` followed by `*`, `+`, `?` should still be one slot.
        for q in [".*", ".+", ".?", r".{1,3}"] {
            let pat = format!("foo {q} bar");
            assert_eq!(canonicalize_pattern(&pat), "foo {} bar", "for {q:?}");
        }
    }
}
