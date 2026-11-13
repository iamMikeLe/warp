//! Author avatars drawn locally: initials on a circle in a color derived from the author's email.

use pathfinder_color::ColorU;
use warp_core::ui::color::pick_foreground_color;
use warpui::elements::{Align, ConstrainedBox, Container, CornerRadius, Element, Radius, Text};
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

#[cfg(test)]
#[path = "avatar_tests.rs"]
mod tests;
