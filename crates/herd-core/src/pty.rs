//! PTY allocation and management utilities.
//!
//! This module wraps `alacritty_terminal`'s PTY functionality with
//! additional helpers for our multi-terminal use case.

use alacritty_terminal::event::WindowSize;

/// Default terminal dimensions
pub const DEFAULT_COLS: u16 = 80;
pub const DEFAULT_ROWS: u16 = 24;
pub const DEFAULT_CELL_WIDTH: u16 = 8;
pub const DEFAULT_CELL_HEIGHT: u16 = 16;

/// Create a default `WindowSize` for initial terminal setup
pub fn default_window_size() -> WindowSize {
    WindowSize {
        num_lines: DEFAULT_ROWS,
        num_cols: DEFAULT_COLS,
        cell_width: DEFAULT_CELL_WIDTH,
        cell_height: DEFAULT_CELL_HEIGHT,
    }
}

/// Compute `WindowSize` from pixel dimensions and cell size
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn default_window_size_returns_expected_dimensions() {
        let ws = default_window_size();
        assert_eq!(ws.num_cols, DEFAULT_COLS);
        assert_eq!(ws.num_lines, DEFAULT_ROWS);
        assert_eq!(ws.cell_width, DEFAULT_CELL_WIDTH);
        assert_eq!(ws.cell_height, DEFAULT_CELL_HEIGHT);
    }

    #[test]
    fn window_size_from_pixels_computes_correct_cols_and_rows() {
        // 640px / 8px per cell = 80 cols, 384px / 16px per cell = 24 rows
        let ws = window_size_from_pixels(640, 384, 8, 16);
        assert_eq!(ws.num_cols, 80);
        assert_eq!(ws.num_lines, 24);
        assert_eq!(ws.cell_width, 8);
        assert_eq!(ws.cell_height, 16);
    }

    #[test]
    fn window_size_from_pixels_non_even_division() {
        // 100px / 8 = 12 (integer division), 50px / 16 = 3
        let ws = window_size_from_pixels(100, 50, 8, 16);
        assert_eq!(ws.num_cols, 12);
        assert_eq!(ws.num_lines, 3);
    }

    #[test]
    fn window_size_from_pixels_handles_zero_cell_width() {
        // cell_width=0 should be clamped to 1 via max(1)
        let ws = window_size_from_pixels(640, 384, 0, 16);
        assert_eq!(ws.num_cols, 640); // 640 / 1
        assert_eq!(ws.num_lines, 24); // 384 / 16
    }

    #[test]
    fn window_size_from_pixels_handles_zero_cell_height() {
        // cell_height=0 should be clamped to 1 via max(1)
        let ws = window_size_from_pixels(640, 384, 8, 0);
        assert_eq!(ws.num_cols, 80); // 640 / 8
        assert_eq!(ws.num_lines, 384); // 384 / 1
    }

    #[test]
    fn window_size_from_pixels_handles_both_zero() {
        let ws = window_size_from_pixels(200, 100, 0, 0);
        assert_eq!(ws.num_cols, 200); // 200 / 1
        assert_eq!(ws.num_lines, 100); // 100 / 1
    }

    #[test]
    fn window_size_from_pixels_zero_pixel_dimensions() {
        let ws = window_size_from_pixels(0, 0, 8, 16);
        assert_eq!(ws.num_cols, 0);
        assert_eq!(ws.num_lines, 0);
    }
}
