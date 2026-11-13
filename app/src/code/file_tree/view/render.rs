use warp_core::ui::theme::ColorScheme;
use warpui::Element as _;
use warpui::assets::asset_cache::AssetSource;
use warpui::elements::{CacheOption, DraggableState, Image, MouseStateHandle};

use super::FileTreeItem;
use crate::appearance::Appearance;
use crate::code::icon_from_file_path;
use crate::code::material_icons::{file_icon_path, folder_icon_path};
use crate::ui_components::icons::Icon;
use crate::ui_components::item_highlight::ImageOrIcon;

fn bundled_image(path: &'static str) -> ImageOrIcon {
    ImageOrIcon::Image(Image::new(AssetSource::Bundled { path }, CacheOption::BySize).finish())
}

fn is_light_theme(appearance: &Appearance) -> bool {
    appearance.theme().inferred_color_scheme() == ColorScheme::DarkOnLight
}

impl FileTreeItem {
    pub(super) fn to_render_state(
        &self,
        is_expanded: Option<bool>,
        appearance: &Appearance,
    ) -> RenderState {
        match self {
            FileTreeItem::File {
                metadata,
                mouse_state_handle,
                depth,
                draggable_state,
            } => {
                let display_name = metadata
                    .path
                    .file_name()
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| String::from("File"));

                let icon = file_icon_path(&display_name, is_light_theme(appearance))
                    .map(bundled_image)
                    .or_else(|| {
                        icon_from_file_path(metadata.path.as_str(), appearance)
                            .map(ImageOrIcon::Image)
                    })
                    .unwrap_or(ImageOrIcon::Icon(Icon::File));

                RenderState {
                    display_name,
                    icon,
                    is_expanded,
                    depth: *depth,
                    mouse_state: mouse_state_handle.clone(),
                    draggable_state: draggable_state.clone(),
                    is_ignored: metadata.ignored,
                }
            }
            FileTreeItem::DirectoryHeader {
                directory,
                mouse_state_handle,
                depth,
                draggable_state,
            } => {
                let display_name = directory
                    .path
                    .file_name()
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| String::from("Folder"));
                let icon = folder_icon_path(
                    &display_name,
                    is_expanded.unwrap_or(false),
                    is_light_theme(appearance),
                )
                .map(bundled_image)
                .unwrap_or(ImageOrIcon::Icon(Icon::Folder));
                RenderState {
                    display_name,
                    icon,
                    is_expanded,
                    depth: *depth,
                    mouse_state: mouse_state_handle.clone(),
                    draggable_state: draggable_state.clone(),
                    is_ignored: directory.ignored,
                }
            }
        }
    }
}

pub(super) struct RenderState {
    pub display_name: String,
    pub icon: ImageOrIcon,
    pub is_expanded: Option<bool>,
    pub depth: usize,
    pub mouse_state: MouseStateHandle,
    pub draggable_state: DraggableState,
    pub is_ignored: bool,
}
