use super::*;

#[test]
fn whole_word_wraps_the_escaped_pattern_in_word_boundaries() {
    let config = SearchConfig {
        use_whole_word: true,
        ..Default::default()
    };
    assert_eq!(config.effective_pattern("a.b"), r"\b(?:a\.b)\b");
    let regex = SearchConfig {
        use_regex: true,
        use_whole_word: true,
        ..Default::default()
    };
    assert_eq!(regex.effective_pattern("ca|dog"), r"\b(?:ca|dog)\b");
    assert_eq!(SearchConfig::default().effective_pattern("a.b"), r"a\.b");
}
