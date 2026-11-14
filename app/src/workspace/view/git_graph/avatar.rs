//! Author avatars: initials on a circle in a color derived from the author's email, optionally
//! replaced by the author's Gravatar picture (opt-in, see [`crate::settings::GitSettings`]).

use pathfinder_color::ColorU;
use sha2::{Digest as _, Sha256};
use warp_core::ui::color::pick_foreground_color;
use warpui::elements::{
    Align, CacheOption, ConstrainedBox, Container, CornerRadius, Element, Image, Radius, Text,
};
use warpui::fonts::{Properties, Weight};

use super::row_canvas::lane_color;
use crate::appearance::Appearance;

/// Up to two uppercase initials: the first letters of the first and last words of `name`, or
/// the first letter of the email when the name is empty.
pub(super) fn initials(name: &str, email: &str) -> String {
    let mut words = name.split_whitespace().filter_map(|w| w.chars().next());
    let first = words.next();
    let last = words.last();
    let letters: String = match (first, last) {
        (Some(first), Some(last)) => [first, last].iter().collect(),
        (Some(first), None) => first.to_string(),
        (None, _) => email.chars().take(1).collect(),
    };
    letters.to_uppercase()
}

/// A stable color per author: the same email always maps to the same palette entry across runs
/// and machines (FNV-1a, not the std hasher, which is randomly seeded).
pub(super) fn author_color(email: &str) -> ColorU {
    let hash = email
        .trim()
        .to_lowercase()
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        });
    lane_color((hash % 64) as usize)
}

pub(super) fn render_initials_avatar(
    name: &str,
    email: &str,
    diameter: f32,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let bg = author_color(email);
    let text = Text::new_inline(
        initials(name, email),
        appearance.ui_font_family(),
        (diameter * 0.42).max(8.),
    )
    .with_color(pick_foreground_color(bg))
    .with_style(Properties::default().weight(Weight::Bold))
    .finish();
    ConstrainedBox::new(
        Container::new(Align::new(text).finish())
            .with_background_color(bg)
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(diameter / 2.)))
            .finish(),
    )
    .with_width(diameter)
    .with_height(diameter)
    .finish()
}

/// Gravatar URL for `email` at `pixels` square. `d=404` makes authors without a Gravatar fail to
/// load, so they keep the initials instead of getting a generic placeholder picture.
pub(super) fn gravatar_url(email: &str, pixels: u32) -> String {
    let hash = Sha256::digest(email.trim().to_lowercase().as_bytes());
    format!("https://gravatar.com/avatar/{hash:x}?s={pixels}&d=404")
}

/// The author's avatar: their Gravatar when `gravatar` is on and one exists, otherwise initials
/// (also shown while the picture loads).
pub(super) fn render_avatar(
    name: &str,
    email: &str,
    diameter: f32,
    gravatar: bool,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let initials = || render_initials_avatar(name, email, diameter, appearance);
    if !gravatar || email.trim().is_empty() {
        return initials();
    }
    // Twice the size for sharp rendering on Retina displays.
    let source = asset_cache::url_source(gravatar_url(email, (diameter * 2.).ceil() as u32));
    ConstrainedBox::new(
        Image::new(source, CacheOption::BySize)
            .cover()
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(diameter / 2.)))
            .before_load(initials())
            .on_load_failure(initials())
            .finish(),
    )
    .with_width(diameter)
    .with_height(diameter)
    .finish()
}

#[cfg(test)]
#[path = "avatar_tests.rs"]
mod tests;
