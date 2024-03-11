use std::{borrow::Cow, fmt::Display};

use once_cell::sync::Lazy;
use regex::{Captures, Regex, Replacer};
use serde::{Deserialize, Serialize};

/// MongoDB identifiers (field names, collection names) can contain characters that are not valid
/// in GraphQL identifiers. These mappings provide GraphQL-safe escape sequences that can be
/// reversed to recover the original MongoDB identifiers.
///
/// CHANGES TO THIS MAPPING ARE API-BREAKING.
///
/// Maps from regular expressions to replacement sequences.
///
/// For invalid characters that do not have mappings here the fallback escape sequence is
/// `__u123D__` where `123D` is replaced with the Unicode codepoint of the escaped character.
///
/// Input sequences of `__` are a special case that are escaped as `____`.
const GRAPHQL_ESCAPE_SEQUENCES: [(char, &str); 2] = [('.', "__dot__"), ('$', "__dollar__")];

/// Make a valid GraphQL name from a string that might contain characters that are not valid in
/// that context. Replaces invalid characters with escape sequences so that the original name can
/// be recovered by reversing the escapes.
///
/// From conversions from string types automatically apply escapes to maintain the invariant that
/// a GqlName is a valid GraphQL name. BUT conversions to strings do not automatically reverse
/// those escape sequences. To recover the original, unescaped name use GqlName::unescape.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub struct GqlName<'a>(Cow<'a, str>);

/// Alias for owned case of GraphQLId
pub type GraphQLName = GqlName<'static>;

impl<'a> GqlName<'a> {
    pub fn from_trusted_safe_string(name: String) -> GraphQLName {
        GqlName(name.into())
    }

    pub fn from_trusted_safe_str(name: &str) -> GqlName<'_> {
        GqlName(name.into())
    }

    /// Replace invalid characters in the given string with escape sequences that are safe in
    /// GraphQL names.
    pub fn escape(name: &str) -> GqlName<'_> {
        // Matches characters that are not alphanumeric or underscores. For the first character of
        // the name the expression is more strict: it does not allow numbers.
        //
        // In addition to invalid characters, this expression replaces sequences of two
        // underscores. We are using two underscores to begin escape sequences, so we need to
        // escape those too.
        static INVALID_SEQUENCES: Lazy<Regex> =
            Lazy::new(|| Regex::new(r"(?:^[^_A-Za-z])|[^_0-9A-Za-z]|__").unwrap());

        let replacement =
            INVALID_SEQUENCES.replace_all(name, |captures: &Captures| -> Cow<'static, str> {
                let sequence = &captures[0];
                if sequence == "__" {
                    return Cow::from("____");
                }
                let char = sequence
                    .chars()
                    .next()
                    .expect("invalid sequence contains a charecter");
                match GRAPHQL_ESCAPE_SEQUENCES
                    .into_iter()
                    .find(|(invalid_char, _)| char == *invalid_char)
                {
                    Some((_, replacement)) => Cow::from(replacement),
                    None => Cow::Owned(format!("__u{:X}__", char as u32)),
                }
            });

        GqlName(replacement)
    }

    /// Replace escape sequences to recover the original name.
    pub fn unescape(self) -> Cow<'a, str> {
        static ESCAPE_SEQUENCE_EXPRESSIONS: Lazy<Regex> = Lazy::new(|| {
            let sequences = GRAPHQL_ESCAPE_SEQUENCES.into_iter().map(|(_, seq)| seq);
            Regex::new(&format!(
                r"(?<underscores>____)|__u(?<codepoint>[0-9A-F]{{1,8}})__|{}",
                itertools::join(sequences, "|")
            ))
            .unwrap()
        });
        ESCAPE_SEQUENCE_EXPRESSIONS.replace_all_cow(self.0, |captures: &Captures| {
            if captures.name("underscores").is_some() {
                "__".to_owned()
            } else if let Some(code_str) = captures.name("codepoint") {
                let code = u32::from_str_radix(code_str.as_str(), 16)
                    .expect("parsing a sequence of 1-8 digits shouldn't fail");
                char::from_u32(code).unwrap().to_string()
            } else {
                let (invalid_char, _) = GRAPHQL_ESCAPE_SEQUENCES
                    .into_iter()
                    .find(|(_, seq)| *seq == &captures[0])
                    .unwrap();
                invalid_char.to_string()
            }
        })
    }

    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }

    /// Clones underlying string only if it's borrowed.
    pub fn into_owned(self) -> GraphQLName {
        GqlName(Cow::Owned(self.0.into_owned()))
    }
}

impl From<String> for GqlName<'static> {
    fn from(value: String) -> Self {
        let inner = match GqlName::escape(&value).0 {
            // If we have a borrowed value then no replacements were made so we can grab the
            // original string instead of allocating a new one.
            Cow::Borrowed(_) => value,
            Cow::Owned(s) => s,
        };
        GqlName(Cow::Owned(inner))
    }
}

impl<'a> From<&'a String> for GqlName<'a> {
    fn from(value: &'a String) -> Self {
        GqlName::escape(value)
    }
}

impl<'a> From<&'a str> for GqlName<'a> {
    fn from(value: &'a str) -> Self {
        GqlName::escape(value)
    }
}

impl<'a> Display for GqlName<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<'a> From<GqlName<'a>> for String {
    fn from(value: GqlName<'a>) -> Self {
        value.0.into_owned()
    }
}

impl<'a, 'b> From<&'b GqlName<'a>> for &'b str {
    fn from(value: &'b GqlName<'a>) -> Self {
        &value.0
    }
}

/// Extension methods for `Regex` that operate on `Cow<str>` instead of `&str`. Avoids allocating
/// new strings on chains of multiple replace calls if no replacements were made.
/// See https://github.com/rust-lang/regex/issues/676#issuecomment-1328973183
trait RegexCowExt {
    /// [`Regex::replace`], but taking text as `Cow<str>` instead of `&str`.
    fn replace_cow<'t, R: Replacer>(&self, text: Cow<'t, str>, rep: R) -> Cow<'t, str>;

    /// [`Regex::replace_all`], but taking text as `Cow<str>` instead of `&str`.
    fn replace_all_cow<'t, R: Replacer>(&self, text: Cow<'t, str>, rep: R) -> Cow<'t, str>;

    /// [`Regex::replacen`], but taking text as `Cow<str>` instead of `&str`.
    fn replacen_cow<'t, R: Replacer>(
        &self,
        text: Cow<'t, str>,
        limit: usize,
        rep: R,
    ) -> Cow<'t, str>;
}

impl RegexCowExt for Regex {
    fn replace_cow<'t, R: Replacer>(&self, text: Cow<'t, str>, rep: R) -> Cow<'t, str> {
        match self.replace(&text, rep) {
            Cow::Owned(result) => Cow::Owned(result),
            Cow::Borrowed(_) => text,
        }
    }

    fn replace_all_cow<'t, R: Replacer>(&self, text: Cow<'t, str>, rep: R) -> Cow<'t, str> {
        match self.replace_all(&text, rep) {
            Cow::Owned(result) => Cow::Owned(result),
            Cow::Borrowed(_) => text,
        }
    }

    fn replacen_cow<'t, R: Replacer>(
        &self,
        text: Cow<'t, str>,
        limit: usize,
        rep: R,
    ) -> Cow<'t, str> {
        match self.replacen(&text, limit, rep) {
            Cow::Owned(result) => Cow::Owned(result),
            Cow::Borrowed(_) => text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::GqlName;

    use pretty_assertions::assert_eq;

    fn assert_escapes(input: &str, expected: &str) {
        let id = GqlName::from(input);
        assert_eq!(id.as_str(), expected);
        assert_eq!(id.unescape(), input);
    }

    #[test]
    fn escapes_invalid_characters() {
        assert_escapes(
            "system.buckets.time_series",
            "system__dot__buckets__dot__time_series",
        );
    }

    #[test]
    fn escapes_runs_of_underscores() {
        assert_escapes("a_____b", "a_________b");
    }

    #[test]
    fn escapes_invalid_with_no_predefined_mapping() {
        assert_escapes("ascii_!", "ascii___u21__");
        assert_escapes("friends♥", "friends__u2665__");
        assert_escapes("👨‍👩‍👧", "__u1F468____u200D____u1F469____u200D____u1F467__");
    }

    #[test]
    fn respects_words_that_appear_in_escape_sequences() {
        assert_escapes("a.dot__", "a__dot__dot____");
        assert_escapes("a.dollar__dot", "a__dot__dollar____dot");
    }

    #[test]
    fn does_not_escape_input_when_deserializing() -> Result<(), anyhow::Error> {
        let input = r#""some__name""#;
        let actual = serde_json::from_str::<GqlName>(input)?;
        assert_eq!(actual.as_str(), "some__name");
        Ok(())
    }

    #[test]
    fn does_not_unescape_input_when_serializing() -> Result<(), anyhow::Error> {
        let output = GqlName::from("system.buckets.time_series");
        let actual = serde_json::to_string(&output)?;
        assert_eq!(
            actual.as_str(),
            r#""system__dot__buckets__dot__time_series""#
        );
        Ok(())
    }
}
