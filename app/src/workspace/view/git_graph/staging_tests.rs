use warpui::elements::{ClippedScrollStateHandle, Empty};

use super::super::data::{ChangedFile, FileStatus};
use super::super::test_support::lay_out;
use super::*;

fn file(path: &str, status: FileStatus, staged: bool) -> ChangedFile {
    ChangedFile {
        path: path.into(),
        additions: 3,
        deletions: 1,
        status,
        staged,
    }
}

/// Both sections (one with files, long paths included, and one empty) and the commit box lay
/// out without a flex panic.
#[test]
fn staging_panel_lays_out() {
    lay_out(|appearance| {
        let files = vec![
            file("README.md", FileStatus::Modified, false),
            file(
                "app/src/a/very/deeply/nested/folder/structure/with_a_long_file_name.rs",
                FileStatus::Added,
                false,
            ),
            file("old.txt", FileStatus::Deleted, true),
        ];
        let mut states = StagingMouseStates::default();
        states.rows = (0..files.len()).map(|_| Default::default()).collect();
        StagingPanel {
            files: &files,
            states: &states,
            summary_input: Empty::new().finish(),
            description_input: Empty::new().finish(),
            summary_len: 80,
            scroll_state: ClippedScrollStateHandle::new(),
        }
        .render(appearance)
    });
    lay_out(|appearance| {
        StagingPanel {
            files: &[],
            states: &StagingMouseStates::default(),
            summary_input: Empty::new().finish(),
            description_input: Empty::new().finish(),
            summary_len: 0,
            scroll_state: ClippedScrollStateHandle::new(),
        }
        .render(appearance)
    });
}
