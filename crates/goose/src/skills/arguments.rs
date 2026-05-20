use crate::utils::split_command_args;
use anyhow::Result;
use regex::{Captures, Regex};
use std::sync::LazyLock;

static PLACEHOLDER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\$ARGUMENTS\[(?P<idx>\d+)\]|\$ARGUMENTS\b|\$(?P<pos>\d+)|\$(?P<name>[A-Za-z_][A-Za-z0-9_-]*)",
    )
    .expect("skill argument regex should compile")
});

fn is_resolvable(caps: &Captures<'_>, names: &[String]) -> bool {
    caps.name("name")
        .map(|m| names.iter().any(|n| n == m.as_str()))
        .unwrap_or(true)
}

pub(super) fn apply_skill_arguments(
    content: &str,
    raw_args: &str,
    argument_names: &[String],
) -> Result<String> {
    if !PLACEHOLDER_RE
        .captures_iter(content)
        .any(|caps| is_resolvable(&caps, argument_names))
    {
        return Ok(format!("{content}\n\nARGUMENTS: {raw_args}"));
    }

    let tokens = split_command_args(raw_args)?;
    let nth = |i: usize| tokens.get(i).cloned().unwrap_or_default();

    let rendered = PLACEHOLDER_RE.replace_all(content, |caps: &Captures<'_>| {
        if let Some(n) = caps.name("idx") {
            return nth(n.as_str().parse().unwrap_or(usize::MAX));
        }
        if let Some(n) = caps.name("pos") {
            let p: usize = n.as_str().parse().unwrap_or(0);
            return p.checked_sub(1).map_or_else(String::new, nth);
        }
        if let Some(name) = caps.name("name") {
            return argument_names
                .iter()
                .position(|n| n == name.as_str())
                .map_or_else(|| caps[0].to_string(), nth);
        }
        raw_args.to_string()
    });

    Ok(rendered.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn arguments_placeholder_is_replaced_with_raw_args() {
        let out = apply_skill_arguments("Run $ARGUMENTS now.", "alpha beta", &[]).unwrap();
        assert_eq!(out, "Run alpha beta now.");
    }

    #[test]
    fn positional_is_one_indexed() {
        let out = apply_skill_arguments("first=$1, second=$2, third=$3", "alpha beta gamma", &[])
            .unwrap();
        assert_eq!(out, "first=alpha, second=beta, third=gamma");
    }

    #[test]
    fn positional_out_of_range_is_empty() {
        let out = apply_skill_arguments("[$5]", "only-one", &[]).unwrap();
        assert_eq!(out, "[]");
    }

    #[test]
    fn arguments_index_is_zero_indexed() {
        let out = apply_skill_arguments(
            "a=$ARGUMENTS[0], b=$ARGUMENTS[1], c=$ARGUMENTS[2]",
            "one two three",
            &[],
        )
        .unwrap();
        assert_eq!(out, "a=one, b=two, c=three");
    }

    #[test]
    fn arguments_index_out_of_range_is_empty() {
        let out = apply_skill_arguments("[$ARGUMENTS[9]]", "one two", &[]).unwrap();
        assert_eq!(out, "[]");
    }

    #[test]
    fn named_arg_maps_to_position() {
        let out = apply_skill_arguments(
            "Migrate $component from $from to $to.",
            "Button old-lib new-lib",
            &names(&["component", "from", "to"]),
        )
        .unwrap();
        assert_eq!(out, "Migrate Button from old-lib to new-lib.");
    }

    #[test]
    fn undeclared_named_arg_stays_literal() {
        let out =
            apply_skill_arguments("Hello $first $missing", "Andy", &names(&["first"])).unwrap();
        assert_eq!(out, "Hello Andy $missing");
    }

    #[test]
    fn quoted_token_groups_multiple_words() {
        let out = apply_skill_arguments(
            "name=$name, addr=$addr",
            r#"Andy "57 Collins""#,
            &names(&["name", "addr"]),
        )
        .unwrap();
        assert_eq!(out, "name=Andy, addr=57 Collins");
    }

    #[test]
    fn apostrophes_in_unquoted_arguments_stay_literal() {
        let out = apply_skill_arguments(
            "author=$author, contraction=$contraction",
            "O'Reilly don't",
            &names(&["author", "contraction"]),
        )
        .unwrap();

        assert_eq!(out, "author=O'Reilly, contraction=don't");
    }

    #[test]
    fn arguments_keeps_raw_quotes_while_positional_strips_them() {
        let out =
            apply_skill_arguments("raw=$ARGUMENTS pos=$1", r#""hello world" tail"#, &[]).unwrap();
        assert_eq!(out, r#"raw="hello world" tail pos=hello world"#);
    }

    #[test]
    fn no_placeholder_appends_arguments_marker() {
        let out = apply_skill_arguments("Just static instructions.", "src/foo.rs", &[]).unwrap();
        assert_eq!(out, "Just static instructions.\n\nARGUMENTS: src/foo.rs");
    }

    #[test]
    fn undeclared_named_only_does_not_count_as_placeholder() {
        let out =
            apply_skill_arguments("Just $missing here.", "src/foo.rs", &names(&["other"])).unwrap();
        assert_eq!(out, "Just $missing here.\n\nARGUMENTS: src/foo.rs");
    }

    #[test]
    fn unmatched_quote_returns_error_when_placeholders_present() {
        let result = apply_skill_arguments("$1", r#""unterminated"#, &[]);
        assert!(result.is_err());
    }

    #[test]
    fn unmatched_quote_is_fine_when_no_placeholders_present() {
        let out = apply_skill_arguments("no placeholders", r#""unterminated"#, &[]).unwrap();
        assert!(out.ends_with(r#"ARGUMENTS: "unterminated"#));
    }

    #[test]
    fn arguments_followed_by_non_index_bracket_is_replaced() {
        let out = apply_skill_arguments("$ARGUMENTS[abc]", "x y", &[]).unwrap();
        assert_eq!(out, "x y[abc]");
    }

    #[test]
    fn extra_tokens_beyond_named_args_reachable_via_positional() {
        let out =
            apply_skill_arguments("$first / $1 / $2 / $3", "a b c", &names(&["first"])).unwrap();
        assert_eq!(out, "a / a / b / c");
    }

    #[test]
    fn declared_name_without_token_substitutes_empty() {
        let out = apply_skill_arguments("[$first][$second]", "only", &names(&["first", "second"]))
            .unwrap();
        assert_eq!(out, "[only][]");
    }

    #[test]
    fn dollar_sign_with_no_match_stays_literal() {
        let out = apply_skill_arguments("price is $100USD or $$ or $", "ignored", &[]).unwrap();
        assert!(out.contains("price is "));
        assert!(out.contains("$$"));
    }

    #[test]
    fn empty_args_with_arguments_placeholder_substitutes_empty() {
        let out = apply_skill_arguments("Run $ARGUMENTS done.", "", &[]).unwrap();
        assert_eq!(out, "Run  done.");
    }

    #[test]
    fn empty_args_with_no_placeholder_still_appends_marker() {
        let out = apply_skill_arguments("Just instructions.", "", &[]).unwrap();
        assert_eq!(out, "Just instructions.\n\nARGUMENTS: ");
    }

    #[test]
    fn repeated_positional_substitutes_every_occurrence() {
        let out = apply_skill_arguments("$1 $1 $1", "alpha", &[]).unwrap();
        assert_eq!(out, "alpha alpha alpha");
    }

    #[test]
    fn multiple_arguments_placeholders_substitute_every_occurrence() {
        let out = apply_skill_arguments("first=$ARGUMENTS / again=$ARGUMENTS", "a b", &[]).unwrap();
        assert_eq!(out, "first=a b / again=a b");
    }

    #[test]
    fn preserves_windows_backslash_paths() {
        let out = apply_skill_arguments("Path: $path", r#""C:\path\""#, &names(&["path"])).unwrap();
        assert_eq!(out, r"Path: C:\path\");
    }

    #[test]
    fn adjacent_positional_placeholders_substitute_independently() {
        let out = apply_skill_arguments("$1$2", "alpha beta", &[]).unwrap();
        assert_eq!(out, "alphabeta");
    }

    #[test]
    fn hyphenated_and_underscored_named_args_resolve() {
        let out = apply_skill_arguments(
            "$my-arg / $_internal",
            "foo bar",
            &names(&["my-arg", "_internal"]),
        )
        .unwrap();
        assert_eq!(out, "foo / bar");
    }
}
