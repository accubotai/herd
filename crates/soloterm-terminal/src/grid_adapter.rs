use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::Term;
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor};

/// A cell ready for rendering with resolved colors
#[derive(Debug, Clone)]
pub struct RenderableCell {
    pub x: usize,
    pub y: usize,
    pub character: char,
    pub fg: CellColor,
    pub bg: CellColor,
    pub flags: CellFlags,
}

/// Simplified color representation for the renderer
#[derive(Debug, Clone, Copy)]
pub enum CellColor {
    Named(NamedColorId),
    Indexed(u8),
    Rgb(u8, u8, u8),
}

/// Named color identifiers (matching standard terminal colors)
#[derive(Debug, Clone, Copy)]
pub enum NamedColorId {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    Foreground,
    Background,
    Cursor,
}

/// Simplified cell flags for rendering
#[derive(Debug, Clone, Copy, Default)]
pub struct CellFlags {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub dim: bool,
    pub inverse: bool,
    pub strikethrough: bool,
    pub hidden: bool,
}

/// Snapshot of terminal content for rendering
#[derive(Debug, Clone)]
pub struct TerminalContent {
    pub cells: Vec<RenderableCell>,
    pub cols: usize,
    pub rows: usize,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub cursor_visible: bool,
}

/// Extract renderable content from an alacritty_terminal::Term
pub fn extract_content<T: alacritty_terminal::event::EventListener>(
    term: &Term<T>,
) -> TerminalContent {
    let grid = term.grid();
    let cols = grid.columns();
    let rows = grid.screen_lines();
    let mut cells = Vec::with_capacity(cols * rows);

    let content = term.renderable_content();

    // display_iter yields Indexed<&Cell> with .point and .cell
    for indexed in content.display_iter {
        let cell = &indexed.cell;
        let renderable = RenderableCell {
            x: indexed.point.column.0,
            y: indexed.point.line.0 as usize,
            character: cell.c,
            fg: convert_color(cell.fg),
            bg: convert_color(cell.bg),
            flags: convert_flags(cell.flags),
        };
        cells.push(renderable);
    }

    // RenderableCursor has .shape and .point (no is_hidden)
    let cursor = content.cursor;
    let cursor_visible = cursor.shape != CursorShape::Hidden;

    TerminalContent {
        cells,
        cols,
        rows,
        cursor_x: cursor.point.column.0,
        cursor_y: cursor.point.line.0 as usize,
        cursor_visible,
    }
}

fn convert_color(color: Color) -> CellColor {
    match color {
        Color::Named(named) => CellColor::Named(convert_named_color(named)),
        Color::Spec(rgb) => CellColor::Rgb(rgb.r, rgb.g, rgb.b),
        Color::Indexed(idx) => CellColor::Indexed(idx),
    }
}

fn convert_named_color(named: NamedColor) -> NamedColorId {
    match named {
        NamedColor::Black => NamedColorId::Black,
        NamedColor::Red => NamedColorId::Red,
        NamedColor::Green => NamedColorId::Green,
        NamedColor::Yellow => NamedColorId::Yellow,
        NamedColor::Blue => NamedColorId::Blue,
        NamedColor::Magenta => NamedColorId::Magenta,
        NamedColor::Cyan => NamedColorId::Cyan,
        NamedColor::White => NamedColorId::White,
        NamedColor::BrightBlack => NamedColorId::BrightBlack,
        NamedColor::BrightRed => NamedColorId::BrightRed,
        NamedColor::BrightGreen => NamedColorId::BrightGreen,
        NamedColor::BrightYellow => NamedColorId::BrightYellow,
        NamedColor::BrightBlue => NamedColorId::BrightBlue,
        NamedColor::BrightMagenta => NamedColorId::BrightMagenta,
        NamedColor::BrightCyan => NamedColorId::BrightCyan,
        NamedColor::BrightWhite => NamedColorId::BrightWhite,
        NamedColor::Foreground => NamedColorId::Foreground,
        NamedColor::Background => NamedColorId::Background,
        NamedColor::Cursor => NamedColorId::Cursor,
        _ => NamedColorId::Foreground,
    }
}

fn convert_flags(flags: Flags) -> CellFlags {
    CellFlags {
        bold: flags.contains(Flags::BOLD),
        italic: flags.contains(Flags::ITALIC),
        underline: flags.contains(Flags::ALL_UNDERLINES),
        dim: flags.contains(Flags::DIM),
        inverse: flags.contains(Flags::INVERSE),
        strikethrough: flags.contains(Flags::STRIKEOUT),
        hidden: flags.contains(Flags::HIDDEN),
    }
}
