//! Tray pictures: prerendered alpha masks (see `build.rs`) tinted at runtime.

use razer_hid::Percent;

/// The notification icon as a 256×256 PNG.
pub const APP_ICON_PNG: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/app-icon.png"));

/// Top of the level fill area on the 24-unit grid.
const FILL_TOP: f32 = 4.0;
/// Height of the level fill area on the 24-unit grid.
const FILL_HEIGHT: f32 = 16.0;
/// Side of the grid the masks are drawn on.
const GRID: f32 = 24.0;
/// Percent value that fills the whole area.
const FULL: f32 = 100.0;

/// Which picture the tray shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Mouse,
    Charging,
    Asleep,
    /// A crossed-out mouse: nothing found. Never shows a level.
    Off,
}

/// The prerendered masks of one shape at one size, `size`×`size` bytes each.
struct MaskSet {
    size: u32,
    outline: &'static [u8],
    /// The area the battery level fills; `None` for shapes that never show a level.
    fill: Option<&'static [u8]>,
    badge: Option<&'static [u8]>,
}

include!(concat!(env!("OUT_DIR"), "/icons.rs"));

/// RGBA pixels of `shape` in `color` with its badge in `badge_color`, filled from the bottom up
/// to `fill`. The side may differ from `wanted`; use the returned one.
pub fn render(
    shape: Shape,
    color: [u8; 3],
    badge_color: [u8; 3],
    fill: Option<Percent>,
    wanted: u32,
) -> (Vec<u8>, u32) {
    let sets = match shape {
        Shape::Mouse => MOUSE,
        Shape::Charging => CHARGING,
        Shape::Asleep => ASLEEP,
        Shape::Off => OFF,
    };
    let set = sets
        .iter()
        .find(|set| set.size >= wanted)
        .or(sets.last())
        .expect("every shape has masks");
    let width = set.size as usize;

    let level = fill.map(Percent::get);
    let area = set.fill.zip(level);
    let mut pixels = Vec::with_capacity(width * width * 4);
    for (index, &outline) in set.outline.iter().enumerate() {
        let body = match area {
            Some((area, level)) => outline.max(scaled(
                area[index],
                row_coverage(index / width, width, level),
            )),
            None => outline,
        };
        // The badge and the body never overlap: the body is cut around the badge.
        let badge = set.badge.map_or(0, |badge| badge[index]);
        let [red, green, blue] = match badge > body {
            true => badge_color,
            false => color,
        };
        pixels.extend_from_slice(&[red, green, blue, body.max(badge)]);
    }
    (pixels, set.size)
}

/// How much of pixel row `row` the fill covers, 0..=1: whole rows below the fill's top edge,
/// a partial row on it.
fn row_coverage(row: usize, side: usize, level: u8) -> f32 {
    let top = (FILL_TOP + FILL_HEIGHT * (1.0 - f32::from(level) / FULL)) / GRID * as_float(side);
    (as_float(row) + 1.0 - top).clamp(0.0, 1.0)
}

#[expect(clippy::cast_precision_loss, reason = "icon sizes are far below 2^24")]
fn as_float(value: usize) -> f32 {
    value as f32
}

/// `value` scaled by `factor` in 0..=1.
fn scaled(value: u8, factor: f32) -> u8 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "result is within 0..=255"
    )]
    let result = (f32::from(value) * factor).round() as u8;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn percent(value: u8) -> Percent {
        Percent::try_from(value).unwrap()
    }

    #[test]
    fn empty_level_equals_no_fill() {
        let plain = render(Shape::Charging, [1, 2, 3], [4, 5, 6], None, 24);
        assert_eq!(
            render(Shape::Charging, [1, 2, 3], [4, 5, 6], Some(percent(0)), 24),
            plain
        );
    }

    #[test]
    fn full_level_covers_the_whole_fill_area() {
        // At 24 px the fill's top edge (grid unit 4) falls exactly on a pixel row.
        let (pixels, side) = render(Shape::Mouse, [0, 0, 0], [0, 0, 0], Some(percent(100)), 24);
        let set = MOUSE.iter().find(|set| set.size == side).unwrap();
        let area = set.fill.unwrap();
        for (index, pixel) in pixels.as_chunks::<4>().0.iter().enumerate() {
            assert_eq!(pixel[3], area[index].max(set.outline[index]));
        }
    }

    #[test]
    fn partial_row_is_antialiased() {
        // At 24 px a grid unit is one pixel, so 50% puts the top edge exactly on row 12.
        assert!((row_coverage(11, 24, 50) - 0.0).abs() < f32::EPSILON);
        assert!((row_coverage(12, 24, 50) - 1.0).abs() < f32::EPSILON);
        // 53% puts it at 11.52: row 11 is covered by 0.48.
        assert!((row_coverage(11, 24, 53) - 0.48).abs() < 0.001);
    }
}
