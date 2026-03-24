//! Terminal color palette — maps terminal color indices to RGB values.
//!
//! Provides both dark and light theme palettes with the standard 16 ANSI
//! colors, 216-color cube, and 24 grayscale colors.

use crate::grid_adapter::{CellColor, NamedColorId};

/// RGB color value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// Terminal color palette with all named and indexed colors.
pub struct Palette {
    pub foreground: Rgb,
    pub background: Rgb,
    pub cursor: Rgb,
    pub named: [Rgb; 16],
}

impl Palette {
    /// Dark theme palette (matches common terminal defaults).
    pub const fn dark() -> Self {
        Self {
            foreground: Rgb::new(204, 204, 204),
            background: Rgb::new(13, 13, 26),
            cursor: Rgb::new(204, 204, 204),
            named: [
                Rgb::new(0, 0, 0),       // Black
                Rgb::new(204, 0, 0),     // Red
                Rgb::new(0, 204, 0),     // Green
                Rgb::new(204, 204, 0),   // Yellow
                Rgb::new(0, 0, 204),     // Blue
                Rgb::new(204, 0, 204),   // Magenta
                Rgb::new(0, 204, 204),   // Cyan
                Rgb::new(204, 204, 204), // White
                Rgb::new(85, 85, 85),    // Bright Black
                Rgb::new(255, 85, 85),   // Bright Red
                Rgb::new(85, 255, 85),   // Bright Green
                Rgb::new(255, 255, 85),  // Bright Yellow
                Rgb::new(85, 85, 255),   // Bright Blue
                Rgb::new(255, 85, 255),  // Bright Magenta
                Rgb::new(85, 255, 255),  // Bright Cyan
                Rgb::new(255, 255, 255), // Bright White
            ],
        }
    }

    /// Resolve a `CellColor` to an RGB value.
    pub fn resolve(&self, color: CellColor) -> Rgb {
        match color {
            CellColor::Named(id) => self.resolve_named(id),
            CellColor::Indexed(idx) => self.resolve_indexed(idx),
            CellColor::Rgb(r, g, b) => Rgb::new(r, g, b),
        }
    }

    fn resolve_named(&self, id: NamedColorId) -> Rgb {
        match id {
            NamedColorId::Black => self.named[0],
            NamedColorId::Red => self.named[1],
            NamedColorId::Green => self.named[2],
            NamedColorId::Yellow => self.named[3],
            NamedColorId::Blue => self.named[4],
            NamedColorId::Magenta => self.named[5],
            NamedColorId::Cyan => self.named[6],
            NamedColorId::White => self.named[7],
            NamedColorId::BrightBlack => self.named[8],
            NamedColorId::BrightRed => self.named[9],
            NamedColorId::BrightGreen => self.named[10],
            NamedColorId::BrightYellow => self.named[11],
            NamedColorId::BrightBlue => self.named[12],
            NamedColorId::BrightMagenta => self.named[13],
            NamedColorId::BrightCyan => self.named[14],
            NamedColorId::BrightWhite => self.named[15],
            NamedColorId::Foreground => self.foreground,
            NamedColorId::Background => self.background,
            NamedColorId::Cursor => self.cursor,
        }
    }

    /// Resolve a 256-color index.
    fn resolve_indexed(&self, idx: u8) -> Rgb {
        match idx {
            // Standard 16 colors
            0..=15 => {
                // Map to named array
                self.named[idx as usize]
            }
            // 216-color cube (indices 16-231)
            16..=231 => {
                let idx = idx - 16;
                let r = idx / 36;
                let g = (idx % 36) / 6;
                let b = idx % 6;
                Rgb::new(
                    if r == 0 { 0 } else { r * 40 + 55 },
                    if g == 0 { 0 } else { g * 40 + 55 },
                    if b == 0 { 0 } else { b * 40 + 55 },
                )
            }
            // 24 grayscale (indices 232-255)
            232..=255 => {
                let v = (idx - 232) * 10 + 8;
                Rgb::new(v, v, v)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dark_palette_foreground() {
        let p = Palette::dark();
        assert_eq!(p.foreground, Rgb::new(204, 204, 204));
    }

    #[test]
    fn test_resolve_named_red() {
        let p = Palette::dark();
        let color = p.resolve(CellColor::Named(NamedColorId::Red));
        assert_eq!(color, Rgb::new(204, 0, 0));
    }

    #[test]
    fn test_resolve_rgb_passthrough() {
        let p = Palette::dark();
        let color = p.resolve(CellColor::Rgb(100, 200, 50));
        assert_eq!(color, Rgb::new(100, 200, 50));
    }

    #[test]
    fn test_resolve_indexed_standard() {
        let p = Palette::dark();
        let color = p.resolve(CellColor::Indexed(1)); // Red
        assert_eq!(color, Rgb::new(204, 0, 0));
    }

    #[test]
    fn test_resolve_indexed_cube() {
        let p = Palette::dark();
        // Index 196 = 5*36 + 0*6 + 0 + 16 → r=5, g=0, b=0 → (255, 0, 0)
        let color = p.resolve(CellColor::Indexed(196));
        assert_eq!(color, Rgb::new(255, 0, 0));
    }

    #[test]
    fn test_resolve_indexed_grayscale() {
        let p = Palette::dark();
        let color = p.resolve(CellColor::Indexed(232)); // Darkest gray
        assert_eq!(color, Rgb::new(8, 8, 8));
        let color = p.resolve(CellColor::Indexed(255)); // Lightest gray
        assert_eq!(color, Rgb::new(238, 238, 238));
    }

    #[test]
    fn test_all_named_colors_resolve() {
        let p = Palette::dark();
        // Just verify none panic
        let ids = [
            NamedColorId::Black,
            NamedColorId::Red,
            NamedColorId::Green,
            NamedColorId::Yellow,
            NamedColorId::Blue,
            NamedColorId::Magenta,
            NamedColorId::Cyan,
            NamedColorId::White,
            NamedColorId::BrightBlack,
            NamedColorId::BrightRed,
            NamedColorId::BrightGreen,
            NamedColorId::BrightYellow,
            NamedColorId::BrightBlue,
            NamedColorId::BrightMagenta,
            NamedColorId::BrightCyan,
            NamedColorId::BrightWhite,
            NamedColorId::Foreground,
            NamedColorId::Background,
            NamedColorId::Cursor,
        ];
        for id in ids {
            let _ = p.resolve(CellColor::Named(id));
        }
    }
}
