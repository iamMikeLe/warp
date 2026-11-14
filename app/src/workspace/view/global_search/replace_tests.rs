use super::*;

fn config(regex: bool, case: bool, word: bool) -> SearchConfig {
    SearchConfig {
        use_regex: regex,
        use_case_sensitivity: case,
        use_whole_word: word,
    }
}

#[test]
fn literal_replace_all_keeps_line_endings_and_ignores_regex_syntax() {
    let replacer = Replacer::new("a.b", config(false, true, false), "$1x").unwrap();
    let (out, count) = replacer
        .apply("a.b and aXb\r\nmore a.b\nend", &ReplaceTarget::All)
        .unwrap();
    assert_eq!(out, "$1x and aXb\r\nmore $1x\nend");
    assert_eq!(count, 2);
}

#[test]
fn regex_mode_expands_capture_groups() {
    let replacer = Replacer::new(r"user_(\w+)", config(true, true, false), "account_$1").unwrap();
    let (out, count) = replacer
        .apply("let user_id = user_name;", &ReplaceTarget::All)
        .unwrap();
    assert_eq!(out, "let account_id = account_name;");
    assert_eq!(count, 2);
}

#[test]
fn whole_word_and_case_insensitive_matching() {
    let replacer = Replacer::new("cat", config(false, false, true), "dog").unwrap();
    let (out, count) = replacer
        .apply("Cat catalog cat concat CAT", &ReplaceTarget::All)
        .unwrap();
    assert_eq!(out, "dog catalog dog concat dog");
    assert_eq!(count, 3);
}

#[test]
fn single_match_replaces_only_that_occurrence() {
    let replacer = Replacer::new("foo", config(false, true, false), "bar").unwrap();
    let content = "foo\nfoo foo foo\nfoo\n";
    let target = ReplaceTarget::One {
        line_number: 2,
        occurrence: 1,
        expected: "foo".into(),
    };
    let (out, count) = replacer.apply(content, &target).unwrap();
    assert_eq!(out, "foo\nfoo bar foo\nfoo\n");
    assert_eq!(count, 1);
}

#[test]
fn single_match_refuses_when_the_text_changed() {
    let replacer = Replacer::new("fo+", config(true, true, false), "x").unwrap();
    let target = ReplaceTarget::One {
        line_number: 1,
        occurrence: 0,
        expected: "foo".into(),
    };
    // The search saw "foo"; the file now has "fooo".
    assert_eq!(replacer.apply("fooo", &target), None);
    // The line or the occurrence is gone.
    let missing = ReplaceTarget::One {
        line_number: 3,
        occurrence: 0,
        expected: "foo".into(),
    };
    assert_eq!(replacer.apply("foo", &missing), None);
}

#[test]
fn multiline_patterns_replace_across_lines_but_not_single_matches() {
    let replacer = Replacer::new("a\nb", config(false, true, false), "c").unwrap();
    assert!(!replacer.supports_single_match());
    let (out, count) = replacer.apply("a\nb\na\nb", &ReplaceTarget::All).unwrap();
    assert_eq!(out, "c\nc");
    assert_eq!(count, 2);
}

#[test]
fn invalid_regex_is_an_error() {
    assert!(Replacer::new("(", config(true, true, false), "x").is_err());
}
