// Hex color literals (0xRRGGBB) are intentionally without separators.
#![allow(clippy::unreadable_literal)]
#![allow(clippy::cast_precision_loss)] // Terminal grid coords (< 500) fit in f32 mantissa
//! GPU-rendered terminal widget using iced's canvas.
//!
//! Draws the terminal grid cell-by-cell with:
//! - Per-cell background colors
//! - Per-cell foreground colors (ANSI 16 + 256 + RGB)
//! - Bold, dim, italic, underline, strikethrough, inverse attributes
//! - Block cursor rendering

use herd_terminal::grid_adapter::{CellFlags, RenderableCell, TerminalContent};
use herd_terminal::palette::{Palette, Rgb};
use iced::mouse;
use iced::widget::canvas::{self, Frame, Geometry};
use iced::{Color, Font, Rectangle, Size, Theme};

/// Cell dimensions in pixels.
const CELL_WIDTH: f32 = 8.4;
const CELL_HEIGHT: f32 = 17.0;
const FONT_SIZE: f32 = 13.5;

/// Terminal rendering program for iced canvas.
///
/// Implements `canvas::Program` — the canvas widget calls `draw()` each frame.
/// All rendering goes through iced's wgpu backend (GPU-accelerated).
pub(crate) struct TerminalProgram {
    pub(crate) content: TerminalContent,
    palette: Palette,
}

impl TerminalProgram {
    pub(crate) fn new(content: TerminalContent) -> Self {
        Self {
            content,
            palette: Palette::dark(),
        }
    }
}

impl canvas::Program<super::app::Message> for TerminalProgram {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry<iced::Renderer>> {
        let mut frame = Frame::new(renderer, bounds.size());

        // Fill background
        frame.fill_rectangle(
            iced::Point::ORIGIN,
            bounds.size(),
            rgb_to_color(self.palette.background),
        );

        // Draw each cell
        for cell in &self.content.cells {
            draw_cell(&mut frame, cell, &self.palette, bounds.size());
        }

        // Draw cursor
        if self.content.cursor_visible {
            let cx = self.content.cursor_x as f32 * CELL_WIDTH;
            let cy = self.content.cursor_y as f32 * CELL_HEIGHT;
            if cx < bounds.width && cy < bounds.height {
                frame.fill_rectangle(
                    iced::Point::new(cx, cy),
                    Size::new(CELL_WIDTH, CELL_HEIGHT),
                    Color {
                        a: 0.7,
                        ..rgb_to_color(self.palette.cursor)
                    },
                );
            }
        }

        vec![frame.into_geometry()]
    }
}

/// Draw a single terminal cell (background rect + character + decorations).
fn draw_cell(frame: &mut Frame, cell: &RenderableCell, palette: &Palette, bounds: Size) {
    let x = cell.x as f32 * CELL_WIDTH;
    let y = cell.y as f32 * CELL_HEIGHT;

    if x > bounds.width || y > bounds.height {
        return;
    }

    let (fg_rgb, bg_rgb) = resolve_colors(palette, cell.fg, cell.bg, cell.flags);

    // Background (skip if matches terminal background)
    if bg_rgb != palette.background {
        frame.fill_rectangle(
            iced::Point::new(x, y),
            Size::new(CELL_WIDTH, CELL_HEIGHT),
            rgb_to_color(bg_rgb),
        );
    }

    // Character
    if cell.character != ' ' && !cell.flags.hidden {
        let font = select_font(cell.flags);
        let mut color = rgb_to_color(fg_rgb);
        if cell.flags.dim {
            color.a = 0.5;
        }

        frame.fill_text(canvas::Text {
            content: cell.character.to_string(),
            position: iced::Point::new(x, y),
            color,
            size: FONT_SIZE.into(),
            font,
            ..canvas::Text::default()
        });
    }

    // Underline
    if cell.flags.underline {
        frame.fill_rectangle(
            iced::Point::new(x, y + CELL_HEIGHT - 2.0),
            Size::new(CELL_WIDTH, 1.0),
            rgb_to_color(fg_rgb),
        );
    }

    // Strikethrough
    if cell.flags.strikethrough {
        frame.fill_rectangle(
            iced::Point::new(x, y + CELL_HEIGHT / 2.0),
            Size::new(CELL_WIDTH, 1.0),
            rgb_to_color(fg_rgb),
        );
    }
}

/// Select font variant based on cell flags.
fn select_font(flags: CellFlags) -> Font {
    match (flags.bold, flags.italic) {
        (true, true) => Font {
            weight: iced::font::Weight::Bold,
            style: iced::font::Style::Italic,
            family: iced::font::Family::Monospace,
            ..Font::MONOSPACE
        },
        (true, false) => Font {
            weight: iced::font::Weight::Bold,
            family: iced::font::Family::Monospace,
            ..Font::MONOSPACE
        },
        (false, true) => Font {
            style: iced::font::Style::Italic,
            family: iced::font::Family::Monospace,
            ..Font::MONOSPACE
        },
        (false, false) => Font::MONOSPACE,
    }
}

/// Resolve foreground/background colors, handling inverse and bold-brightening.
fn resolve_colors(
    palette: &Palette,
    fg: herd_terminal::grid_adapter::CellColor,
    bg: herd_terminal::grid_adapter::CellColor,
    flags: CellFlags,
) -> (Rgb, Rgb) {
    let mut fg_rgb = palette.resolve(fg);
    let mut bg_rgb = palette.resolve(bg);

    if flags.inverse {
        std::mem::swap(&mut fg_rgb, &mut bg_rgb);
    }
    if flags.bold {
        fg_rgb = brighten(fg_rgb);
    }

    (fg_rgb, bg_rgb)
}

fn rgb_to_color(rgb: Rgb) -> Color {
    Color::from_rgb8(rgb.r, rgb.g, rgb.b)
}

fn brighten(rgb: Rgb) -> Rgb {
    Rgb::new(
        rgb.r.saturating_add(30),
        rgb.g.saturating_add(30),
        rgb.b.saturating_add(30),
    )
}
