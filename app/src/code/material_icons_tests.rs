use super::*;

fn assert_bundled(path: &str) {
    assert!(
        crate::ASSETS.get(path).is_ok(),
        "{path} is referenced by the manifest but not bundled"
    );
}

#[test]
fn common_files_get_language_icons() {
    assert_eq!(
        file_icon_path("main.rs", false),
        Some("bundled/svg/material/rust.svg")
    );
    assert_eq!(
        file_icon_path("App.tsx", false),
        Some("bundled/svg/material/react_ts.svg")
    );
    assert_eq!(
        file_icon_path("README.md", false),
        Some("bundled/svg/material/readme.svg")
    );
}

#[test]
fn exact_file_name_beats_extension() {
    let package_json = file_icon_path("package.json", false).unwrap();
    assert_ne!(package_json, file_icon_path("other.json", false).unwrap());
    assert_bundled(package_json);
}

#[test]
fn longest_extension_wins() {
    let spec = file_icon_path("widget.spec.ts", false).unwrap();
    assert_ne!(spec, file_icon_path("widget.ts", false).unwrap());
}

#[test]
fn unknown_files_and_folders_get_generic_icons() {
    assert_eq!(
        file_icon_path("no-such-kind.zzzz", false),
        Some("bundled/svg/material/file.svg")
    );
    assert_eq!(
        folder_icon_path("no-such-folder-xyz", false, false),
        Some("bundled/svg/material/folder.svg")
    );
    assert_eq!(
        folder_icon_path("no-such-folder-xyz", true, false),
        Some("bundled/svg/material/folder-open.svg")
    );
}

#[test]
fn named_folders_have_open_and_closed_icons() {
    let closed = folder_icon_path("src", false, false).unwrap();
    let open = folder_icon_path("src", true, false).unwrap();
    assert_ne!(closed, "bundled/svg/material/folder.svg");
    assert_ne!(closed, open);
    assert_bundled(closed);
    assert_bundled(open);
}

#[test]
fn every_mapped_icon_is_bundled() {
    let icons = ICONS.as_ref().expect("manifest loads");
    for path in icons.paths.values() {
        assert_bundled(path);
    }
}
