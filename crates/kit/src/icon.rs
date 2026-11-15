//! One rounded-keycap style for executable and standalone tray icons.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Normal,
    Suspended,
    Error,
    Disabled,
    Success,
    SuspendedSuccess,
    SuspendedError,
}

#[derive(Clone, Copy)]
pub struct Spec {
    pub letter: char,
    pub fill: [u8; 4],
    pub edge: [u8; 4],
}

impl Spec {
    const fn new(letter: char, fill: [u8; 3], edge: [u8; 3]) -> Self {
        Self {
            letter,
            fill: [fill[0], fill[1], fill[2], 255],
            edge: [edge[0], edge[1], edge[2], 255],
        }
    }
}

pub const DESKTOP: Spec = Spec::new('D', [37, 99, 235], [30, 64, 175]);
pub const WINDOW: Spec = Spec::new('W', [37, 99, 235], [30, 64, 175]);
pub const SCREEN: Spec = Spec::new('S', [8, 145, 178], [21, 94, 117]);
pub const TASKBAR: Spec = Spec::new('T', [80, 160, 255], [32, 64, 112]);
pub const KEYBOARD_SWITCH: Spec = Spec::new('K', [59, 110, 165], [35, 70, 110]);
pub const KEYBOARD_SHELL: Spec = Spec::new('S', [106, 81, 163], [66, 50, 102]);
pub const KEYBOARD_KEYMAP: Spec = Spec::new('K', [106, 81, 163], [66, 50, 102]);
pub const KEYBOARD_LOCKER: Spec = Spec::new('L', [46, 125, 50], [27, 77, 29]);
pub const KEYBOARD_AUTO_SWITCH_BACK: Spec = Spec::new('A', [59, 110, 165], [35, 70, 110]);
pub const GOLDENDICT_SEARCH: Spec = Spec::new('G', [184, 134, 11], [124, 89, 6]);
pub const GOLDENDICT_AUTO_SEARCH: Spec = Spec::new('G', [184, 134, 11], [124, 89, 6]);

/// Executable name and shared identity; the asset generator consumes this list.
pub const PRODUCTS: &[(&str, Spec)] = &[
    ("desktop-tools", DESKTOP),
    ("window-tools", WINDOW),
    ("screen-tools", SCREEN),
    ("taskbar-tools", TASKBAR),
    ("keyboard-switch", KEYBOARD_SWITCH),
    ("keyboard-shell", KEYBOARD_SHELL),
    ("keyboard-keymap", KEYBOARD_KEYMAP),
    ("keyboard-locker", KEYBOARD_LOCKER),
    ("keyboard-auto-switch-back", KEYBOARD_AUTO_SWITCH_BACK),
    ("goldendict-search-in-group", GOLDENDICT_SEARCH),
    ("goldendict-auto-search", GOLDENDICT_AUTO_SEARCH),
];

pub fn rgba(spec: Spec, state: State) -> Vec<u8> {
    draw(spec, state, 32)
}

fn draw(spec: Spec, state: State, n: i32) -> Vec<u8> {
    let mut c = Canvas::new(n);
    let grey = [107, 114, 128, 255];
    let (fill, edge) = match state {
        State::Suspended | State::Disabled | State::SuspendedSuccess | State::SuspendedError => {
            (grey, [75, 85, 99, 255])
        }
        _ => (spec.fill, spec.edge),
    };
    for y in 0..32 {
        for x in 0..32 {
            let corner = |cx: i32, cy: i32| (x - cx).pow(2) + (y - cy).pow(2) <= 16;
            let inside = (2..30).contains(&x) && (2..30).contains(&y)
                || corner(6, 6)
                || corner(25, 6)
                || corner(6, 25)
                || corner(25, 25);
            if inside {
                let border = !(4..28).contains(&x) || !(4..28).contains(&y);
                c.rect(
                    x as f32,
                    y as f32,
                    (x + 1) as f32,
                    (y + 1) as f32,
                    if border { edge } else { fill },
                );
            }
        }
    }
    if let Some(glyph) = glyph(spec.letter) {
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..5 {
                if bits >> (4 - col) & 1 == 1 {
                    let x = 9 + col * 3;
                    let y = 5 + row * 3;
                    c.rect(x as f32, y as f32, (x + 3) as f32, (y + 3) as f32, WHITE);
                }
            }
        }
    }
    match state {
        State::Error | State::SuspendedError => c.badge(RED),
        State::Success | State::SuspendedSuccess => c.badge(YELLOW),
        State::Disabled => {
            for i in 0..=18 {
                c.disc(7.0 + i as f32, 25.0 - i as f32, 1.25, grey);
            }
        }
        _ => {}
    }
    c.downsample()
}

fn glyph(ch: char) -> Option<[u8; 7]> {
    Some(match ch.to_ascii_uppercase() {
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'G' => [
            0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010,
        ],
        _ => return None,
    })
}

/// BMP-DIB ICO entries at every size used by the tray, Explorer and high DPI.
pub fn ico(spec: Spec, state: State) -> Vec<u8> {
    const SIZES: [u32; 6] = [16, 20, 24, 32, 48, 256];
    let images: Vec<Vec<u8>> = SIZES.iter().map(|&n| draw(spec, state, n as i32)).collect();
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0, SIZES.len() as u8, 0]);
    let mut offset = (6 + 16 * SIZES.len()) as u32;
    for &n in &SIZES {
        let mask_row = n.div_ceil(32) * 4;
        let size = 40 + n * n * 4 + mask_row * n;
        let encoded = if n == 256 { 0 } else { n as u8 };
        out.extend_from_slice(&[encoded, encoded, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += size;
    }
    for (i, &n) in SIZES.iter().enumerate() {
        let px = &images[i];
        let mask_row = (n.div_ceil(32) * 4) as usize;
        out.extend_from_slice(&40u32.to_le_bytes());
        out.extend_from_slice(&n.to_le_bytes());
        out.extend_from_slice(&(n * 2).to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&[0; 24]);
        for y in (0..n).rev() {
            for x in 0..n {
                let at = ((y * n + x) * 4) as usize;
                out.extend_from_slice(&[px[at + 2], px[at + 1], px[at], px[at + 3]]);
            }
        }
        out.resize(out.len() + mask_row * n as usize, 0);
    }
    out
}

const WHITE: [u8; 4] = [255, 255, 255, 255];
const YELLOW: [u8; 4] = [245, 179, 1, 255];
const RED: [u8; 4] = [220, 38, 38, 255];
const RING: [u8; 4] = [31, 41, 55, 255];
const SS: i32 = 4;
const AUTHORED: i32 = 32;

struct Canvas {
    n: i32,
    px: Vec<[u8; 4]>,
}

impl Canvas {
    fn new(n: i32) -> Self {
        Self {
            n,
            px: vec![[0; 4]; (n * SS * n * SS) as usize],
        }
    }

    fn set(&mut self, x: i32, y: i32, color: [u8; 4]) {
        let w = self.n * SS;
        if (0..w).contains(&x) && (0..w).contains(&y) {
            self.px[(y * w + x) as usize] = color;
        }
    }

    fn scale(&self) -> f32 {
        SS as f32 * self.n as f32 / AUTHORED as f32
    }

    fn rect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: [u8; 4]) {
        let s = self.scale();
        for y in (y0 * s).round() as i32..(y1 * s).round() as i32 {
            for x in (x0 * s).round() as i32..(x1 * s).round() as i32 {
                self.set(x, y, color);
            }
        }
    }

    fn disc(&mut self, cx: f32, cy: f32, r: f32, color: [u8; 4]) {
        let s = self.scale();
        for y in ((cy - r) * s).floor() as i32..((cy + r) * s).ceil() as i32 {
            for x in ((cx - r) * s).floor() as i32..((cx + r) * s).ceil() as i32 {
                let px = (x as f32 + 0.5) / s;
                let py = (y as f32 + 0.5) / s;
                if (px - cx).powi(2) + (py - cy).powi(2) <= r * r {
                    self.set(x, y, color);
                }
            }
        }
    }

    fn badge(&mut self, color: [u8; 4]) {
        self.disc(24.0, 24.0, 5.4, RING);
        self.disc(24.0, 24.0, 4.0, color);
    }

    fn downsample(self) -> Vec<u8> {
        let n = self.n;
        let samples = (SS * SS) as u32;
        let mut out = vec![0u8; (n * n * 4) as usize];
        for y in 0..n {
            for x in 0..n {
                let (mut r, mut g, mut b, mut a) = (0u32, 0u32, 0u32, 0u32);
                for dy in 0..SS {
                    for dx in 0..SS {
                        let p = self.px[((y * SS + dy) * n * SS + x * SS + dx) as usize];
                        let alpha = p[3] as u32;
                        r += p[0] as u32 * alpha;
                        g += p[1] as u32 * alpha;
                        b += p[2] as u32 * alpha;
                        a += alpha;
                    }
                }
                let i = ((y * n + x) * 4) as usize;
                let alpha = a.max(1);
                out[i] = (r / alpha) as u8;
                out[i + 1] = (g / alpha) as u8;
                out[i + 2] = (b / alpha) as u8;
                out[i + 3] = (a / samples) as u8;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_product_and_state_draws_a_valid_distinct_icon() {
        for &(_, spec) in PRODUCTS {
            let normal = rgba(spec, State::Normal);
            assert_eq!(normal.len(), 32 * 32 * 4);
            assert_eq!(normal[3], 0, "margin stays transparent");
            for state in [
                State::Suspended,
                State::Error,
                State::Disabled,
                State::Success,
            ] {
                assert_ne!(normal, rgba(spec, state), "{:?} must be visible", state);
            }
        }
    }

    #[test]
    fn ico_has_six_coherent_entries_including_256() {
        let icon = ico(WINDOW, State::Normal);
        assert_eq!(&icon[0..4], &[0, 0, 1, 0]);
        assert_eq!(icon[4], 6);
        let mut expected_offset = (6 + 16 * 6) as u32;
        for (i, size) in [16u8, 20, 24, 32, 48, 0].iter().enumerate() {
            let at = 6 + i * 16;
            assert_eq!(icon[at], *size);
            assert_eq!(icon[at + 1], *size);
            assert_eq!(&icon[at + 4..at + 6], &1u16.to_le_bytes());
            assert_eq!(&icon[at + 6..at + 8], &32u16.to_le_bytes());
            let size = u32::from_le_bytes(icon[at + 8..at + 12].try_into().unwrap());
            let offset = u32::from_le_bytes(icon[at + 12..at + 16].try_into().unwrap());
            assert_eq!(offset, expected_offset);
            expected_offset += size;
        }
        assert_eq!(expected_offset, icon.len() as u32);
    }

    #[test]
    fn every_executable_asset_matches_the_shared_renderer() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for &(exe, spec) in PRODUCTS {
            let crate_dir = match exe {
                "desktop-tools" => "apps/desktop-tools".into(),
                "window-tools" => "domains/window/window-tools".into(),
                "screen-tools" => "domains/screen/screen-tools".into(),
                "taskbar-tools" => "domains/taskbar/taskbar-tools".into(),
                _ => format!("plugins/{exe}"),
            };
            let path = root
                .join(crate_dir)
                .join("assets")
                .join(format!("{exe}.ico"));
            assert_eq!(
                std::fs::read(&path).unwrap(),
                ico(spec, State::Normal),
                "{} is stale; run cargo run -p kit --example icons",
                path.display()
            );
        }
    }
}
