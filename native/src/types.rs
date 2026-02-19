//! Shared types, enums, and constants.
//!
//! All types that cross module boundaries or define the FFI data model live here.

#[allow(unused)]
use bitflags::bitflags;

// ============================================================================
// Node Types
// ============================================================================

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    Box = 0,
    Text = 1,
    Input = 2,
    Select = 3,
    ScrollBox = 4,
}

impl NodeType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Box),
            1 => Some(Self::Text),
            2 => Some(Self::Input),
            3 => Some(Self::Select),
            4 => Some(Self::ScrollBox),
            _ => None,
        }
    }

    /// Whether this node type is a leaf (cannot have children).
    pub fn is_leaf(self) -> bool {
        matches!(self, Self::Text | Self::Input | Self::Select)
    }
}

// ============================================================================
// Color Encoding (u32)
// ============================================================================
//
// Bits 31-24: Mode tag
//   0x00 = Default (terminal default)
//   0x01 = RGB truecolor (bits 23-0 = 0xRRGGBB)
//   0x02 = Indexed (bits 7-0 = palette index 0-255)

#[allow(dead_code)]
pub const COLOR_DEFAULT: u32 = 0x00000000;

pub fn color_tag(color: u32) -> u8 {
    ((color >> 24) & 0xFF) as u8
}

pub fn color_to_crossterm(color: u32) -> Option<crossterm::style::Color> {
    match color_tag(color) {
        0x00 => None, // Default — no override
        0x01 => {
            let r = ((color >> 16) & 0xFF) as u8;
            let g = ((color >> 8) & 0xFF) as u8;
            let b = (color & 0xFF) as u8;
            Some(crossterm::style::Color::Rgb { r, g, b })
        }
        0x02 => {
            let index = (color & 0xFF) as u8;
            Some(crossterm::style::Color::AnsiValue(index))
        }
        _ => None, // Invalid tag — treat as Default
    }
}

// ============================================================================
// Border Style
// ============================================================================

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderStyle {
    None = 0,
    Single = 1,
    Double = 2,
    Rounded = 3,
    Bold = 4,
}

impl BorderStyle {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::None),
            1 => Some(Self::Single),
            2 => Some(Self::Double),
            3 => Some(Self::Rounded),
            4 => Some(Self::Bold),
            _ => None,
        }
    }

    /// Returns the border characters: (top-left, top-right, bottom-left, bottom-right, horizontal, vertical)
    pub fn chars(self) -> Option<(char, char, char, char, char, char)> {
        match self {
            Self::None => None,
            Self::Single => Some(('┌', '┐', '└', '┘', '─', '│')),
            Self::Double => Some(('╔', '╗', '╚', '╝', '═', '║')),
            Self::Rounded => Some(('╭', '╮', '╰', '╯', '─', '│')),
            Self::Bold => Some(('┏', '┓', '┗', '┛', '━', '┃')),
        }
    }
}

// ============================================================================
// Cell Attributes (bitflags)
// ============================================================================

bitflags! {
    #[repr(transparent)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct CellAttrs: u8 {
        const BOLD      = 0b0000_0001;
        const ITALIC    = 0b0000_0010;
        const UNDERLINE = 0b0000_0100;
    }
}

// ============================================================================
// Content Format
// ============================================================================

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentFormat {
    Plain = 0,
    Markdown = 1,
    Code = 2,
}

impl ContentFormat {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Plain),
            1 => Some(Self::Markdown),
            2 => Some(Self::Code),
            _ => None,
        }
    }
}

// ============================================================================
// Event Types
// ============================================================================

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiEventType {
    None = 0,
    Key = 1,
    Mouse = 2,
    Resize = 3,
    FocusChange = 4,
    Change = 5,
    Submit = 6,
}

/// FFI-safe event struct. Fixed layout, 24 bytes.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TuiEvent {
    pub event_type: u32,
    pub target: u32,
    pub data: [u32; 4],
}

impl TuiEvent {
    pub fn none() -> Self {
        Self {
            event_type: TuiEventType::None as u32,
            target: 0,
            data: [0; 4],
        }
    }

    pub fn key(target: u32, key_code: u32, modifiers: u32, codepoint: u32) -> Self {
        Self {
            event_type: TuiEventType::Key as u32,
            target,
            data: [key_code, modifiers, codepoint, 0],
        }
    }

    pub fn mouse(target: u32, x: u32, y: u32, button: u32, modifiers: u32) -> Self {
        Self {
            event_type: TuiEventType::Mouse as u32,
            target,
            data: [x, y, button, modifiers],
        }
    }

    pub fn resize(width: u32, height: u32) -> Self {
        Self {
            event_type: TuiEventType::Resize as u32,
            target: 0,
            data: [width, height, 0, 0],
        }
    }

    pub fn focus_change(from: u32, to: u32) -> Self {
        Self {
            event_type: TuiEventType::FocusChange as u32,
            target: 0,
            data: [from, to, 0, 0],
        }
    }

    pub fn change(target: u32, data0: u32) -> Self {
        Self {
            event_type: TuiEventType::Change as u32,
            target,
            data: [data0, 0, 0, 0],
        }
    }

    pub fn submit(target: u32) -> Self {
        Self {
            event_type: TuiEventType::Submit as u32,
            target,
            data: [0; 4],
        }
    }
}

// ============================================================================
// Key Code Constants
// ============================================================================

pub mod key {
    pub const BACKSPACE: u32 = 0x0100;
    pub const ENTER: u32 = 0x0101;
    pub const LEFT: u32 = 0x0102;
    pub const RIGHT: u32 = 0x0103;
    pub const UP: u32 = 0x0104;
    pub const DOWN: u32 = 0x0105;
    pub const HOME: u32 = 0x0106;
    pub const END: u32 = 0x0107;
    pub const PAGE_UP: u32 = 0x0108;
    pub const PAGE_DOWN: u32 = 0x0109;
    pub const TAB: u32 = 0x010A;
    pub const BACK_TAB: u32 = 0x010B;
    pub const DELETE: u32 = 0x010C;
    pub const INSERT: u32 = 0x010D;
    pub const ESCAPE: u32 = 0x010E;
    pub const F1: u32 = 0x0110;
}

#[allow(dead_code)]
pub mod modifier {
    pub const SHIFT: u32 = 0x01;
    pub const CTRL: u32 = 0x02;
    pub const ALT: u32 = 0x04;
    pub const SUPER: u32 = 0x08;
}

// ============================================================================
// Visual Style
// ============================================================================

#[derive(Debug, Clone)]
pub struct VisualStyle {
    pub fg_color: u32,
    pub bg_color: u32,
    pub border_style: BorderStyle,
    pub border_color: u32,
    pub attrs: CellAttrs,
    pub opacity: f32,
    pub style_mask: u8,
}

impl VisualStyle {
    pub const MASK_FG_COLOR: u8 = 0b0000_0001; // bit 0
    pub const MASK_BG_COLOR: u8 = 0b0000_0010; // bit 1
    pub const MASK_BORDER_COLOR: u8 = 0b0000_0100; // bit 2
    pub const MASK_BORDER_STYLE: u8 = 0b0000_1000; // bit 3
    pub const MASK_ATTRS: u8 = 0b0001_0000; // bit 4
    pub const MASK_OPACITY: u8 = 0b0010_0000; // bit 5
    pub const MASK_ALL: u8 = 0b0011_1111;
}

impl Default for VisualStyle {
    fn default() -> Self {
        Self {
            fg_color: 0,
            bg_color: 0,
            border_style: BorderStyle::None,
            border_color: 0,
            attrs: CellAttrs::empty(),
            opacity: 1.0,
            style_mask: 0,
        }
    }
}

// ============================================================================
// Cell & Buffer
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub ch: char,
    pub fg: u32,
    pub bg: u32,
    pub attrs: CellAttrs,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: 0,
            bg: 0,
            attrs: CellAttrs::empty(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Buffer {
    pub width: u16,
    pub height: u16,
    pub cells: Vec<Cell>,
}

impl Buffer {
    pub fn new(width: u16, height: u16) -> Self {
        let size = (width as usize) * (height as usize);
        Self {
            width,
            height,
            cells: vec![Cell::default(); size],
        }
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        let size = (width as usize) * (height as usize);
        self.cells.resize(size, Cell::default());
        self.clear();
    }

    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            *cell = Cell::default();
        }
    }

    pub fn get(&self, x: u16, y: u16) -> Option<&Cell> {
        if x < self.width && y < self.height {
            Some(&self.cells[(y as usize) * (self.width as usize) + (x as usize)])
        } else {
            None
        }
    }

    pub fn set(&mut self, x: u16, y: u16, cell: Cell) {
        if x < self.width && y < self.height {
            self.cells[(y as usize) * (self.width as usize) + (x as usize)] = cell;
        }
    }
}

// ============================================================================
// Cell Update (for TerminalBackend trait)
// ============================================================================

#[derive(Debug, Clone)]
pub struct CellUpdate {
    pub x: u16,
    pub y: u16,
    pub cell: Cell,
}

// ============================================================================
// Terminal Input Event (internal, not FFI)
// ============================================================================

#[derive(Debug, Clone)]
pub enum TerminalInputEvent {
    Key {
        code: u32,
        modifiers: u32,
        character: char,
    },
    Mouse {
        x: u16,
        y: u16,
        button: u8,
        modifiers: u32,
    },
    Resize {
        width: u16,
        height: u16,
    },
    FocusGained,
    FocusLost,
}

// ============================================================================
// Styled Span (for Text Module output)
// ============================================================================

#[derive(Debug, Clone)]
pub struct StyledSpan {
    pub text: String,
    pub attrs: CellAttrs,
    pub fg: u32,
    pub bg: u32,
}

// ============================================================================
// TuiNode
// ============================================================================

#[derive(Debug, Clone)]
pub struct TuiNode {
    pub node_type: NodeType,
    pub taffy_node: taffy::NodeId,
    pub content: String,
    pub content_format: ContentFormat,
    pub code_language: Option<String>,
    pub children: Vec<u32>,
    pub parent: Option<u32>,
    pub visual_style: VisualStyle,
    pub dirty: bool,
    pub focusable: bool,
    pub visible: bool,
    pub scroll_x: i32,
    pub scroll_y: i32,
    // Input widget state
    pub cursor_position: u32,
    pub max_length: u32,
    pub mask_char: u32,
    // Select widget state
    pub options: Vec<String>,
    pub selected_index: Option<u32>,
}

impl TuiNode {
    pub fn new(node_type: NodeType, taffy_node: taffy::NodeId) -> Self {
        let focusable = matches!(node_type, NodeType::Input | NodeType::Select);
        Self {
            node_type,
            taffy_node,
            content: String::new(),
            content_format: ContentFormat::Plain,
            code_language: None,
            children: Vec::new(),
            parent: None,
            visual_style: VisualStyle::default(),
            dirty: true,
            focusable,
            visible: true,
            scroll_x: 0,
            scroll_y: 0,
            cursor_position: 0,
            max_length: 0,
            mask_char: 0,
            options: Vec::new(),
            selected_index: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_type_from_u8() {
        assert_eq!(NodeType::from_u8(0), Some(NodeType::Box));
        assert_eq!(NodeType::from_u8(1), Some(NodeType::Text));
        assert_eq!(NodeType::from_u8(4), Some(NodeType::ScrollBox));
        assert_eq!(NodeType::from_u8(5), None);
    }

    #[test]
    fn test_color_encoding() {
        assert!(color_to_crossterm(COLOR_DEFAULT).is_none());

        let red_rgb = 0x01FF0000;
        match color_to_crossterm(red_rgb) {
            Some(crossterm::style::Color::Rgb { r, g, b }) => {
                assert_eq!((r, g, b), (255, 0, 0));
            }
            other => panic!("expected Rgb, got {:?}", other),
        }

        let ansi_1 = 0x02000001;
        match color_to_crossterm(ansi_1) {
            Some(crossterm::style::Color::AnsiValue(1)) => {}
            other => panic!("expected AnsiValue(1), got {:?}", other),
        }

        // Invalid tag falls back to None
        assert!(color_to_crossterm(0x03000000).is_none());
    }

    #[test]
    fn test_cell_attrs_bitflags() {
        let mut attrs = CellAttrs::empty();
        attrs |= CellAttrs::BOLD;
        attrs |= CellAttrs::UNDERLINE;
        assert!(attrs.contains(CellAttrs::BOLD));
        assert!(!attrs.contains(CellAttrs::ITALIC));
        assert!(attrs.contains(CellAttrs::UNDERLINE));
    }

    #[test]
    fn test_tui_event_size() {
        assert_eq!(std::mem::size_of::<TuiEvent>(), 24);
    }

    #[test]
    fn test_buffer_operations() {
        let mut buf = Buffer::new(10, 5);
        assert_eq!(buf.cells.len(), 50);

        buf.set(
            3,
            2,
            Cell {
                ch: 'X',
                fg: 0,
                bg: 0,
                attrs: CellAttrs::empty(),
            },
        );
        assert_eq!(buf.get(3, 2).unwrap().ch, 'X');
        assert_eq!(buf.get(0, 0).unwrap().ch, ' ');
        assert!(buf.get(10, 5).is_none());
    }

    #[test]
    fn test_border_style_chars() {
        assert!(BorderStyle::None.chars().is_none());
        let (tl, _tr, _bl, _br, h, v) = BorderStyle::Single.chars().unwrap();
        assert_eq!(tl, '┌');
        assert_eq!(h, '─');
        assert_eq!(v, '│');
    }
}
