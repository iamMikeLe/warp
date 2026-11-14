use settings::macros::define_settings_group;
use settings::{RespectUserSyncSetting, SupportedPlatforms, SyncToCloud};

// Git-related settings. For now this only controls the visibility of the Git Graph panel;
// future git-related options (such as default branch filtering or connector style) belong in
// this group too, surfaced through the dedicated "Git" tab in settings.
define_settings_group!(GitSettings, settings: [
    // User preference: when FeatureFlag::GitGraph is enabled for the current channel, controls
    // whether the Git Graph panel appears in the left tools panel (a tab visibility toggle of the
    // same kind as show_project_explorer / show_global_search).
    show_git_graph: ShowGitGraph {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        surface: settings::SettingSurfaces::GUI,
        private: false,
        toml_path: "git.show_graph_panel",
        description: "Whether the Git Graph panel is shown in the tools panel.",
    },
    // Whether stashes appear in the Git Graph, each attached directly above its base commit.
    show_git_graph_stashes: ShowGitGraphStashes {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        surface: settings::SettingSurfaces::GUI,
        private: false,
        toml_path: "git.show_stashes",
        description: "Whether stashes are shown in the Git Graph, above the commit they were made on.",
    },
    // Opt-in: show authors' Gravatar pictures on Git Graph nodes and in commit details. Off by
    // default because it sends a hash of each visible author's email to gravatar.com; when off (or
    // when an author has no Gravatar) the avatars show locally drawn initials.
    git_graph_gravatar: GitGraphGravatar {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        surface: settings::SettingSurfaces::GUI,
        private: false,
        toml_path: "git.graph_gravatar",
        description: "Whether the Git Graph loads author pictures from Gravatar (sends a hash of each author's email to gravatar.com).",
    },
    // Not user-visible: the Git Graph's Branch / Tag and Graph column widths, remembered across
    // restarts. Kept per device because the right widths depend on the screen.
    git_graph_refs_column_width: GitGraphRefsColumnWidth {
        type: f32,
        default: 170.0,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        surface: settings::SettingSurfaces::GUI,
        private: true,
    },
    git_graph_graph_column_width: GitGraphGraphColumnWidth {
        type: f32,
        default: 160.0,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        surface: settings::SettingSurfaces::GUI,
        private: true,
    },
    // How many directory levels below the working directory the Git Graph probes for repositories:
    // 0 = only the repository the working directory itself belongs to; 1 (default) = also check
    // whether each first-level subdirectory is a repository; 2/3 descend further. Useful when a
    // single directory hosts several independent git projects; when multiple repositories are found,
    // a repository dropdown appears at the top of the panel to switch between them.
    git_graph_scan_depth: GitGraphScanDepth {
        type: u32,
        default: 1,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        surface: settings::SettingSurfaces::GUI,
        private: false,
        toml_path: "git.graph_scan_depth",
        description: "How many directory levels below the working directory the Git Graph scans for repositories (0 = working directory only).",
    },
]);
