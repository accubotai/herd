//! PTY allocation and management utilities.
//!
//! This module wraps alacritty_terminal's PTY functionality with
//! additional helpers for our multi-terminal use case.

use alacritty_terminal::event::WindowSize;

/// Default terminal dimensions
pub const DEFAULT_COLS: u16 = 80;
pub const DEFAULT_ROWS: u16 = 24;
pub const DEFAULT_CELL_WIDTH: u16 = 8;
pub const DEFAULT_CELL_HEIGHT: u16 = 16;

/// Create a default WindowSize for initial terminal setup
pub fn default_window_size() -> WindowSize {
    WindowSize {
        num_lines: DEFAULT_ROWS,
        num_cols: DEFAULT_COLS,
        cell_width: DEFAULT_CELL_WIDTH,
        cell_height: DEFAULT_CELL_HEIGHT,
    }
}

/// Compute WindowSize from pixel dimensions and cell size
pub fn window_size_from_pixels(
    width_px: u16,
    height_px: u16,
    cell_width: u16,
    cell_height: u16,
) -> WindowSize {
    WindowSize {
        num_cols: width_px / cell_width.max(1),
        num_lines: height_px / cell_height.max(1),
        cell_width,
        cell_height,
    }
}
