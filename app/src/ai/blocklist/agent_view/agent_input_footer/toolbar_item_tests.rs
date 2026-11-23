use super::*;

/// The File explorer chip is offered to the agent view toolbelt editor.
#[test]
fn file_explorer_is_offered_in_the_agent_view() {
    assert!(
        AgentToolbarItemKind::FileExplorer
            .available_in()
            .is_available_for_agent_view()
    );
    assert!(AgentToolbarItemKind::all_available().contains(&AgentToolbarItemKind::FileExplorer));
}

/// ...but it is opt-in, so it must stay out of the default agent view layout.
#[test]
fn file_explorer_is_not_an_agent_view_default() {
    assert!(!AgentToolbarItemKind::default_left().contains(&AgentToolbarItemKind::FileExplorer));
    assert!(!AgentToolbarItemKind::default_right().contains(&AgentToolbarItemKind::FileExplorer));
}

/// Polish Prompt rewrites the CLI agent rich input, so it only belongs to the CLI agent toolbar.
#[test]
fn polish_prompt_is_cli_agent_only() {
    let availability = AgentToolbarItemKind::PolishPrompt.available_in();
    assert!(availability.is_available_for_cli());
    assert!(!availability.is_available_for_agent_view());
    assert!(
        AgentToolbarItemKind::all_available_for_cli_input()
            .contains(&AgentToolbarItemKind::PolishPrompt)
    );
    assert!(!AgentToolbarItemKind::all_available().contains(&AgentToolbarItemKind::PolishPrompt));
}
