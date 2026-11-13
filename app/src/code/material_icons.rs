//! File and folder icons from the bundled Material Icon Theme (MIT; see
//! `assets/bundled/svg/material/LICENSE`), regenerated with `script/update_material_icons.py`.

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;
use warpui::AssetProvider as _;

const MANIFEST_PATH: &str = "bundled/material_icons.json";

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Mappings {
    file_extensions: HashMap<String, String>,
    file_names: HashMap<String, String>,
    folder_names: HashMap<String, String>,
    folder_names_expanded: HashMap<String, String>,
}

/// Values are icon file stems.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    file: String,
    folder: String,
    folder_expanded: String,
    #[serde(flatten)]
    dark: Mappings,
    light: Mappings,
}

/// The manifest plus every icon's asset path, built once so lookups can hand out `&'static str`
/// paths (which `AssetSource::Bundled` requires).
struct Icons {
    manifest: Manifest,
    paths: HashMap<String, String>,
}

impl Icons {
    fn load() -> Option<Self> {
        let bytes = crate::ASSETS.get(MANIFEST_PATH).ok()?;
        let manifest: Manifest = serde_json::from_slice(&bytes)
            .inspect_err(|err| log::warn!("Failed to parse {MANIFEST_PATH}: {err}"))
            .ok()?;
        let stems = [&manifest.file, &manifest.folder, &manifest.folder_expanded]
            .into_iter()
            .chain([&manifest.dark, &manifest.light].into_iter().flat_map(|m| {
                m.file_extensions
                    .values()
                    .chain(m.file_names.values())
                    .chain(m.folder_names.values())
                    .chain(m.folder_names_expanded.values())
            }));
        let paths = stems
            .map(|stem| (stem.clone(), format!("bundled/svg/material/{stem}.svg")))
            .collect();
        Some(Self { manifest, paths })
    }

    fn path(&'static self, stem: &str) -> Option<&'static str> {
        self.paths.get(stem).map(String::as_str)
    }

    /// `light` mappings take precedence when the theme is light, falling back to the dark ones.
    fn lookup(
        &self,
        light: bool,
        pick: impl Fn(&Mappings) -> &HashMap<String, String>,
        key: &str,
    ) -> Option<&String> {
        light
            .then(|| pick(&self.manifest.light).get(key))
            .flatten()
            .or_else(|| pick(&self.manifest.dark).get(key))
    }
}

static ICONS: LazyLock<Option<Icons>> = LazyLock::new(Icons::load);

/// Asset path of the icon for a file named `file_name`: an exact (case-insensitive) file name
/// match first, then the longest matching extension (`test.ts` before `ts`), then the generic file
/// icon. `None` only if the bundled manifest is missing or invalid.
pub fn file_icon_path(file_name: &str, light: bool) -> Option<&'static str> {
    let icons = ICONS.as_ref()?;
    let name = file_name.to_lowercase();
    let by_name = icons.lookup(light, |m| &m.file_names, &name);
    let by_extension = || {
        name.match_indices('.')
            .map(|(dot, _)| &name[dot + 1..])
            .find_map(|extension| icons.lookup(light, |m| &m.file_extensions, extension))
    };
    let stem = by_name
        .or_else(by_extension)
        .unwrap_or(&icons.manifest.file);
    icons.path(stem)
}

/// Asset path of the icon for a folder named `folder_name`, open or closed, falling back to the
/// generic folder icon. `None` only if the bundled manifest is missing or invalid.
pub fn folder_icon_path(folder_name: &str, expanded: bool, light: bool) -> Option<&'static str> {
    let icons = ICONS.as_ref()?;
    let name = folder_name.to_lowercase();
    let stem = if expanded {
        icons
            .lookup(light, |m| &m.folder_names_expanded, &name)
            .unwrap_or(&icons.manifest.folder_expanded)
    } else {
        icons
            .lookup(light, |m| &m.folder_names, &name)
            .unwrap_or(&icons.manifest.folder)
    };
    icons.path(stem)
}

#[cfg(test)]
#[path = "material_icons_tests.rs"]
mod tests;
