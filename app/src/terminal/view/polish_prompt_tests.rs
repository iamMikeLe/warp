use super::*;

#[test]
fn clean_polished_output_trims_whitespace() {
    assert_eq!(
        clean_polished_output("\n\n  Fix the bug.\n\n").as_deref(),
        Some("Fix the bug.")
    );
}

#[test]
fn clean_polished_output_unwraps_a_whole_answer_fence() {
    let output = "```markdown\nFix the bug.\n\n## Context\n- `app/src/main.rs`\n```\n";
    assert_eq!(
        clean_polished_output(output).as_deref(),
        Some("Fix the bug.\n\n## Context\n- `app/src/main.rs`")
    );
}

#[test]
fn clean_polished_output_keeps_inner_code_fences() {
    let output = "Fix the bug.\n\n```sh\ncargo test\n```";
    assert_eq!(clean_polished_output(output).as_deref(), Some(output));
}

#[test]
fn clean_polished_output_keeps_text_with_several_fences() {
    let output = "```rust\nfn a() {}\n```\nThen:\n```rust\nfn b() {}\n```";
    assert_eq!(clean_polished_output(output).as_deref(), Some(output));
}

#[test]
fn clean_polished_output_rejects_empty_output() {
    assert_eq!(clean_polished_output("  \n "), None);
    assert_eq!(clean_polished_output("```\n```"), None);
}

#[test]
fn polish_input_wraps_the_trimmed_draft() {
    assert_eq!(
        polish_input("  fix the login bug \n"),
        "Rewrite the draft prompt inside the <draft> tags.\n\n<draft>\nfix the login bug\n</draft>"
    );
}

#[test]
fn polish_args_run_headless_without_tools_or_persistence() {
    let args = polish_args();
    assert_eq!(args.first(), Some(&"-p"));
    let value_after = |flag: &str| {
        args.iter()
            .position(|arg| *arg == flag)
            .and_then(|index| args.get(index + 1))
            .copied()
    };
    assert_eq!(value_after("--tools"), Some(""));
    assert_eq!(value_after("--setting-sources"), Some(""));
    assert_eq!(value_after("--system-prompt"), Some(POLISH_SYSTEM_PROMPT));
    assert!(args.contains(&"--no-session-persistence"));
}

#[test]
fn failure_reason_prefers_the_first_stderr_line() {
    assert_eq!(
        failure_reason("\n  Invalid API key\nmore\n", "ignored"),
        "Invalid API key"
    );
}

#[test]
fn failure_reason_falls_back_to_stdout_then_a_generic_message() {
    assert_eq!(failure_reason("", "Not logged in\n"), "Not logged in");
    assert_eq!(failure_reason(" \n", ""), "`claude` exited with an error.");
}
