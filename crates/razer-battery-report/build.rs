//! Renders the tray icon masks and the app icon from the Lucide sources, and embeds the licence
//! notices, the Windows app icon and the Windows manifest.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::{env, fs};

use resvg::{tiny_skia, usvg};

/// Tray icon sizes: 16 px at 100% display scaling, 20 at 125%, 24 at 150%, 32 at 200% and up.
const SIZES: [u32; 7] = [16, 20, 24, 32, 40, 48, 64];
/// Side of the Lucide grid, in grid units.
const GRID: f32 = 24.0;
/// Width of the badge (zap, moon) relative to the grid.
const BADGE_SCALE: f32 = 0.42;
/// Centre of the badge on the grid.
const BADGE_CENTER: (f32, f32) = (18.5, 18.5);
/// Clear space around the badge, in grid units, cut out of the mouse.
const GAP: f32 = 1.25;
/// Stroke width of every Lucide icon, in grid units.
const STROKE: f32 = 2.0;
/// Whether the badge is drawn solid instead of as an outline.
const BADGE_SOLID: bool = false;
/// Level fill area, a rounded rectangle one unit inside the mouse outline.
const FILL_AREA: &str = "<rect x='7' y='4' width='10' height='16' rx='5'/>";
/// The scroll wheel of the mouse; cut out of the level fill.
const WHEEL: &str = "<path d='M12 6v4'/>";

/// Side of the notification icon, in pixels.
const APP_ICON_SIZE: u32 = 256;
/// Sides of the images in `app.ico`, in pixels.
const APP_ICON_SIZES: [u32; 8] = [16, 20, 24, 32, 40, 48, 64, APP_ICON_SIZE];
/// Background colour of the notification icon.
const APP_ICON_BACKGROUND: &str = "#2563EB";
/// Corner radius of the notification icon, as a fraction of its side.
const APP_ICON_RADIUS: f32 = 0.23;
/// Padding around the mouse in the notification icon, as a fraction of its side.
const APP_ICON_PADDING: f32 = 0.10;

/// The app icon in all sizes; the installer script finds it under this name in `OUT_DIR`.
const APP_ICO: &str = "app.ico";
/// The Windows manifest that is embedded into the executable.
const MANIFEST: &str = "app.manifest";
/// Resource type id of a manifest.
const RT_MANIFEST: u32 = 24;
/// Size of the `ICONDIR` header of an `.ico` file.
const ICO_HEADER_BYTES: usize = 6;
/// Size of one `ICONDIRENTRY` of an `.ico` file.
const ICO_ENTRY_BYTES: usize = 16;
/// Type field of an `.ico` header; a `.cur` file has 2.
const ICO_TYPE_ICON: u16 = 1;
/// Bits per pixel recorded for each image.
const ICO_BITS_PER_PIXEL: u16 = 32;
/// Width or height byte of an entry that stands for 256 pixels.
const ICO_SIDE_256: u8 = 0;
/// Path of the generated licence notices; the release workflow sets it.
const NOTICES_ENV: &str = "THIRD_PARTY_NOTICES";
/// Directory of the Lucide sources, relative to the crate.
const LUCIDE_DIR: &str = "../../assets/icons/lucide";

/// A symbol drawn in the bottom right corner of the mouse.
#[derive(Clone, Copy)]
enum Badge {
    Zap,
    Moon,
}

/// A tray picture: the Rust name of its constant, the base icon and an optional badge.
struct Shape {
    name: &'static str,
    base: &'static str,
    badge: Option<Badge>,
}

const SHAPES: [Shape; 4] = [
    Shape {
        name: "MOUSE",
        base: "mouse",
        badge: None,
    },
    Shape {
        name: "CHARGING",
        base: "mouse",
        badge: Some(Badge::Zap),
    },
    Shape {
        name: "ASLEEP",
        base: "mouse",
        badge: Some(Badge::Moon),
    },
    Shape {
        name: "OFF",
        base: "mouse-off",
        badge: None,
    },
];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={MANIFEST}");
    println!("cargo:rerun-if-changed={LUCIDE_DIR}");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    let sources = Sources::load();
    render_icons(&sources, &out_dir);
    render_app_icon(&sources, &out_dir);
    let ico = out_dir.join(APP_ICO);
    embed_notices(&out_dir);

    // A console exe lets shells wait for the CLI; the manifest stops Windows 11 24H2+ from
    // opening a console for the tray. Older Windows ignore it and the tray frees the console.
    if env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows") {
        embed_resources(&out_dir, &ico);
    }
}

/// The inner SVG markup of the Lucide icons used here.
struct Sources {
    mouse: String,
    mouse_off: String,
    zap: String,
    moon: String,
}

impl Sources {
    /// Reads the icons from `assets/icons/lucide/` and registers them for rebuilds.
    fn load() -> Self {
        let read = |name: &str| {
            let path = Path::new(LUCIDE_DIR).join(format!("{name}.svg"));
            println!("cargo:rerun-if-changed={}", path.display());
            let svg = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
            body(&svg)
        };
        Self {
            mouse: read("mouse"),
            mouse_off: read("mouse-off"),
            zap: read("zap"),
            moon: read("moon"),
        }
    }

    fn base(&self, name: &str) -> &str {
        match name {
            "mouse-off" => &self.mouse_off,
            _ => &self.mouse,
        }
    }

    fn badge(&self, badge: Badge) -> &str {
        match badge {
            Badge::Zap => &self.zap,
            Badge::Moon => &self.moon,
        }
    }
}

/// Renders every shape at every size and writes the masks and `icons.rs`.
fn render_icons(sources: &Sources, out_dir: &Path) {
    let mut code = String::new();
    for shape in &SHAPES {
        let badge = shape.badge.map(|badge| sources.badge(badge));
        let base = sources.base(shape.base);
        let outline = outline_svg(base, badge);
        let badge_svg = badge.map(badge_svg);
        // The crossed-out mouse never shows a level.
        let level_svg = badge.map_or_else(
            || (shape.base == "mouse").then(|| fill_svg(None)),
            |badge| Some(fill_svg(Some(badge))),
        );

        writeln!(
            code,
            "/// Prerendered masks of one shape.\nconst {}: &[MaskSet] = &[",
            shape.name
        )
        .unwrap();
        for size in SIZES {
            let stem = format!("{}-{size}", shape.name.to_lowercase());
            let outline_file = format!("{stem}-outline.bin");
            fs::write(out_dir.join(&outline_file), alpha(&outline, size))
                .expect("cannot write mask");
            let level_code = match &level_svg {
                Some(svg) => {
                    let name = format!("{stem}-level.bin");
                    fs::write(out_dir.join(&name), alpha(svg, size)).expect("cannot write mask");
                    format!("Some(include_bytes!(concat!(env!(\"OUT_DIR\"), \"/{name}\")))")
                }
                None => "None".to_owned(),
            };
            let badge_code = match &badge_svg {
                Some(svg) => {
                    let name = format!("{stem}-badge.bin");
                    fs::write(out_dir.join(&name), alpha(svg, size)).expect("cannot write mask");
                    format!("Some(include_bytes!(concat!(env!(\"OUT_DIR\"), \"/{name}\")))")
                }
                None => "None".to_owned(),
            };
            writeln!(
                code,
                "    MaskSet {{ size: {size}, outline: include_bytes!(concat!(env!(\"OUT_DIR\"), \"/{outline_file}\")), fill: {level_code}, badge: {badge_code} }},"
            )
            .unwrap();
        }
        code.push_str("];\n\n");
    }
    fs::write(out_dir.join("icons.rs"), code).expect("cannot write icons.rs");
}

/// Renders the app icon: a white mouse on a blue rounded square. Writes `app-icon.png` for the
/// notifications and [`APP_ICO`] with every size of [`APP_ICON_SIZES`].
fn render_app_icon(sources: &Sources, out_dir: &Path) {
    let images: Vec<(u32, Vec<u8>)> = APP_ICON_SIZES
        .iter()
        .map(|&size| (size, app_icon_png(sources, size)))
        .collect();
    let notification = images
        .iter()
        .find(|(size, _)| *size == APP_ICON_SIZE)
        .expect("the sizes include the notification size");
    fs::write(out_dir.join("app-icon.png"), &notification.1).expect("cannot write the app icon");
    fs::write(out_dir.join(APP_ICO), ico(&images)).expect("cannot write app.ico");
}

/// The app icon as a PNG of `size` pixels.
fn app_icon_png(sources: &Sources, size: u32) -> Vec<u8> {
    let edge = pixels(size);
    let radius = edge * APP_ICON_RADIUS;
    let scale = edge * (1.0 - 2.0 * APP_ICON_PADDING) / GRID;
    let offset = edge * APP_ICON_PADDING;
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 {edge} {edge}'>         <rect width='{edge}' height='{edge}' rx='{radius}' fill='{APP_ICON_BACKGROUND}'/>         <g transform='translate({offset} {offset}) scale({scale})' fill='none' stroke='white'          stroke-width='{STROKE}' stroke-linecap='round' stroke-linejoin='round'>{}</g></svg>",
        sources.mouse
    );
    rasterize(&svg, size)
        .encode_png()
        .expect("cannot encode the app icon")
}

/// An `.ico` file from square PNG images, each given with its side in pixels.
fn ico(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let count = u16::try_from(images.len()).expect("few images");
    let mut file = Vec::new();
    file.extend_from_slice(&0_u16.to_le_bytes());
    file.extend_from_slice(&ICO_TYPE_ICON.to_le_bytes());
    file.extend_from_slice(&count.to_le_bytes());
    let mut offset = ICO_HEADER_BYTES + ICO_ENTRY_BYTES * images.len();
    for (size, png) in images {
        let side = u8::try_from(*size).unwrap_or(ICO_SIDE_256);
        file.extend_from_slice(&[side, side, 0, 0]);
        file.extend_from_slice(&1_u16.to_le_bytes());
        file.extend_from_slice(&ICO_BITS_PER_PIXEL.to_le_bytes());
        file.extend_from_slice(&u32::try_from(png.len()).expect("small image").to_le_bytes());
        file.extend_from_slice(&u32::try_from(offset).expect("small file").to_le_bytes());
        offset += png.len();
    }
    for (_, png) in images {
        file.extend_from_slice(png);
    }
    file
}

/// Compiles a resource script with the icon (id 1) and the manifest into the executable.
fn embed_resources(out_dir: &Path, ico: &Path) {
    let manifest =
        Path::new(&env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets it")).join(MANIFEST);
    let quoted = |path: &Path| path.display().to_string().replace('\\', "\\\\");
    let script = format!(
        "1 ICON \"{}\"\n1 {RT_MANIFEST} \"{}\"\n",
        quoted(ico),
        quoted(&manifest)
    );
    let path = out_dir.join("app.rc");
    fs::write(&path, script).expect("cannot write the resource script");
    embed_resource::compile(&path, embed_resource::NONE)
        .manifest_required()
        .expect("cannot embed the Windows resources");
}

/// Copies the licence notices named by [`NOTICES_ENV`] for the exe to embed. Builds without
/// them, local and pull request ones, embed a pointer to the releases instead.
fn embed_notices(out_dir: &Path) {
    println!("cargo:rerun-if-env-changed={NOTICES_ENV}");
    let target = out_dir.join("THIRD-PARTY.md");
    let Some(path) = env::var_os(NOTICES_ENV).map(PathBuf::from) else {
        let repository = env!("CARGO_PKG_REPOSITORY");
        let placeholder = format!(
            "# Third-party licenses\n\n\
             Razer Battery Report is open source and available on GitHub: {repository}\n\n\
             License: {repository}/blob/master/LICENSE\n\n\
             Licence notices are included in release builds: {repository}/releases\n"
        );
        fs::write(&target, placeholder).expect("cannot write the licence notices");
        return;
    };
    println!("cargo:rerun-if-changed={}", path.display());
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    fs::write(&target, text).expect("cannot write the licence notices");
}

/// The SVG of the mouse outline in black, cut around the badge; only its alpha is used.
fn outline_svg(base: &str, badge: Option<&str>) -> String {
    let mut defs = String::new();
    let mut shapes = String::new();
    let cut = badge.map(cut_mask);
    if let Some(cut) = &cut {
        defs.push_str(cut);
    }
    let masked = cut.map_or(String::new(), |_| " mask='url(#cut)'".to_owned());
    write!(
        shapes,
        "<g{masked} fill='none' stroke='black' stroke-width='{STROKE}' stroke-linecap='round' stroke-linejoin='round'>{base}</g>"
    )
    .unwrap();
    wrap(&defs, &shapes)
}

/// The SVG of the level fill area in black: the inset rectangle minus the scroll wheel and the
/// space around the badge.
fn fill_svg(badge: Option<&str>) -> String {
    let wheel_width = STROKE + 2.0 * GAP;
    let mut defs = format!(
        "<mask id='wheel' maskUnits='userSpaceOnUse' x='0' y='0' width='{GRID}' height='{GRID}'>\
         <rect width='{GRID}' height='{GRID}' fill='white'/>\
         <g fill='none' stroke='black' stroke-width='{wheel_width}' stroke-linecap='round'>{WHEEL}</g></mask>"
    );
    let masked = match badge {
        Some(badge) => {
            defs.push_str(&cut_mask(badge));
            " mask='url(#cut)'"
        }
        None => "",
    };
    let shapes =
        format!("<g{masked}><g mask='url(#wheel)'><g fill='black'>{FILL_AREA}</g></g></g>");
    wrap(&defs, &shapes)
}

/// A mask that is white everywhere except around the badge.
fn cut_mask(badge: &str) -> String {
    let width = (STROKE + 2.0 * GAP) / BADGE_SCALE;
    let fill = match BADGE_SOLID {
        true => "black",
        false => "none",
    };
    format!(
        "<mask id='cut' maskUnits='userSpaceOnUse' x='0' y='0' width='{GRID}' height='{GRID}'>\
         <rect width='{GRID}' height='{GRID}' fill='white'/>\
         <g transform='{}' stroke='black' fill='{fill}' stroke-width='{width}' stroke-linecap='round' \
         stroke-linejoin='round'>{badge}</g></mask>",
        badge_transform()
    )
}

/// The SVG of the badge alone in black, drawn in its own colour at runtime.
fn badge_svg(badge: &str) -> String {
    let width = STROKE / BADGE_SCALE;
    let fill = match BADGE_SOLID {
        true => "black",
        false => "none",
    };
    let badge = format!(
        "<g transform='{}' fill='{fill}' stroke='black' stroke-width='{width}' stroke-linecap='round' \
         stroke-linejoin='round'>{badge}</g>",
        badge_transform()
    );
    wrap("", &badge)
}

/// Moves and scales the badge's own 24-unit grid to its place on the mouse.
fn badge_transform() -> String {
    let (cx, cy) = BADGE_CENTER;
    let half = GRID / 2.0 * BADGE_SCALE;
    format!(
        "translate({} {}) scale({BADGE_SCALE})",
        cx - half,
        cy - half
    )
}

fn wrap(defs: &str, shapes: &str) -> String {
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 {GRID} {GRID}'><defs>{defs}</defs>{shapes}</svg>"
    )
}

/// The alpha channel of `svg` rendered at `size` pixels, row by row.
fn alpha(svg: &str, size: u32) -> Vec<u8> {
    rasterize(svg, size)
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| pixel[3])
        .collect()
}

/// A pixel count as a float; icon sizes are small, so the conversion is exact.
fn pixels(size: u32) -> f32 {
    f32::from(u16::try_from(size).expect("icon size fits in u16"))
}

fn rasterize(svg: &str, size: u32) -> tiny_skia::Pixmap {
    let tree =
        usvg::Tree::from_str(svg, &usvg::Options::default()).expect("generated SVG is invalid");
    let mut pixmap = tiny_skia::Pixmap::new(size, size).expect("icon size is not zero");
    let scale = pixels(size) / tree.size().width();
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap
}

/// The markup between the outer `<svg …>` tag and `</svg>`. Lucide puts the closing `>` of the
/// opening tag on its own line.
fn body(svg: &str) -> String {
    let svg = svg.replace("\r\n", "\n");
    let start = svg.find("\n>").expect("unexpected Lucide SVG layout") + 2;
    let end = svg.rfind("</svg>").expect("SVG without a closing tag");
    svg[start..end].trim().to_owned()
}
