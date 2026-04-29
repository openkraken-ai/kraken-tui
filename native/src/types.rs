//! Shared types, enums, and constants.
//!
//! All types that cross module boundaries or define the FFI data model live here.

use serde::Serialize;
use std::collections::{HashMap, VecDeque};

#[allow(unused)]
use bitflags::bitflags;

// ============================================================================
// Node Types
// ============================================================================

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum NodeType {
    Box = 0,
    Text = 1,
    Input = 2,
    Select = 3,
    ScrollBox = 4,
    TextArea = 5,
    Table = 6,
    List = 7,
    Tabs = 8,
    Overlay = 9,
    Transcript = 10,
    SplitPane = 11,
}

impl NodeType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Box),
            1 => Some(Self::Text),
            2 => Some(Self::Input),
            3 => Some(Self::Select),
            4 => Some(Self::ScrollBox),
            5 => Some(Self::TextArea),
            6 => Some(Self::Table),
            7 => Some(Self::List),
            8 => Some(Self::Tabs),
            9 => Some(Self::Overlay),
            10 => Some(Self::Transcript),
            11 => Some(Self::SplitPane),
            _ => None,
        }
    }

    /// Whether this node type is a leaf (cannot have children).
    pub fn is_leaf(self) -> bool {
        matches!(
            self,
            Self::Text
                | Self::Input
                | Self::Select
                | Self::TextArea
                | Self::Table
                | Self::List
                | Self::Tabs
                | Self::Transcript
        )
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
        const BOLD          = 0b0000_0001;
        const ITALIC        = 0b0000_0010;
        const UNDERLINE     = 0b0000_0100;
        const STRIKETHROUGH = 0b0000_1000;
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
    Accessibility = 7,
}

// ============================================================================
// Accessibility Role (ADR-T23)
// ============================================================================

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessibilityRole {
    Button = 0,
    Checkbox = 1,
    Input = 2,
    TextArea = 3,
    List = 4,
    ListItem = 5,
    Heading = 6,
    Region = 7,
    Status = 8,
}

impl AccessibilityRole {
    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            0 => Some(Self::Button),
            1 => Some(Self::Checkbox),
            2 => Some(Self::Input),
            3 => Some(Self::TextArea),
            4 => Some(Self::List),
            5 => Some(Self::ListItem),
            6 => Some(Self::Heading),
            7 => Some(Self::Region),
            8 => Some(Self::Status),
            _ => None,
        }
    }
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

    pub fn accessibility(target: u32, role_code: u32) -> Self {
        Self {
            event_type: TuiEventType::Accessibility as u32,
            target,
            data: [role_code, 0, 0, 0],
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
// Animation Enums (v1)
// ============================================================================

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimProp {
    Opacity = 0,
    FgColor = 1,
    BgColor = 2,
    BorderColor = 3,
    PositionX = 4,
    PositionY = 5,
}

impl AnimProp {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Opacity),
            1 => Some(Self::FgColor),
            2 => Some(Self::BgColor),
            3 => Some(Self::BorderColor),
            4 => Some(Self::PositionX),
            5 => Some(Self::PositionY),
            _ => None,
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Easing {
    Linear = 0,
    EaseIn = 1,
    EaseOut = 2,
    EaseInOut = 3,
    CubicIn = 4,
    CubicOut = 5,
    Elastic = 6,
    Bounce = 7,
}

impl Easing {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Linear),
            1 => Some(Self::EaseIn),
            2 => Some(Self::EaseOut),
            3 => Some(Self::EaseInOut),
            4 => Some(Self::CubicIn),
            5 => Some(Self::CubicOut),
            6 => Some(Self::Elastic),
            7 => Some(Self::Bounce),
            _ => None,
        }
    }
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
// Text Cache (ADR-T25)
// ============================================================================

/// Cache key for parsed text content.
///
/// Invalidation mapping:
/// - Content change → content_hash differs → cache miss
/// - Format change → format differs → cache miss
/// - Language change → language_hash differs → cache miss
/// - Wrap width change → wrap_width differs → cache miss (reserved for future pre-wrap caching)
/// - Style fingerprint change → style_fingerprint differs → cache miss (syntect theme for Code)
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct TextCacheKey {
    pub content_hash: u64,
    pub format: u8,
    pub language_hash: u64,
    pub wrap_width: u16,
    pub style_fingerprint: u64,
}

/// Cached parse result for a text content entry.
#[derive(Debug, Clone)]
pub struct TextCacheEntry {
    pub spans: Vec<StyledSpan>,
    pub byte_size: u32,
    pub last_access_tick: u64,
}

/// Bounded LRU cache for parsed text content.
///
/// Capacity is hard bounded at `max_bytes` (default 8 MiB).
/// Eviction is LRU by access tick, tracked via `lru_order`.
/// Memory accounting: `used_bytes` never exceeds `max_bytes`.
pub struct TextCache {
    pub entries: std::collections::HashMap<TextCacheKey, TextCacheEntry>,
    pub lru_order: std::collections::VecDeque<TextCacheKey>,
    pub max_bytes: u32,
    pub used_bytes: u32,
    pub tick: u64,
}

impl TextCache {
    pub fn new(max_bytes: u32) -> Self {
        Self {
            entries: std::collections::HashMap::new(),
            lru_order: std::collections::VecDeque::new(),
            max_bytes,
            used_bytes: 0,
            tick: 0,
        }
    }
}

impl Default for TextCache {
    fn default() -> Self {
        Self::new(8_388_608) // 8 MiB
    }
}

// ============================================================================
// v3 Widget State (ADR-T27)
// ============================================================================

#[derive(Debug, Clone)]
pub struct TableColumn {
    pub label: String,
    pub width_value: u16,
    pub width_unit: u8, // 0=fixed, 1=percent, 2=flex
}

#[derive(Debug, Clone)]
pub struct TableState {
    pub columns: Vec<TableColumn>,
    pub rows: Vec<Vec<String>>,
    pub selected_row: Option<u32>,
    pub header_visible: bool,
}

impl Default for TableState {
    fn default() -> Self {
        Self {
            columns: Vec::new(),
            rows: Vec::new(),
            selected_row: None,
            header_visible: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ListState {
    pub items: Vec<String>,
    pub selected: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct TabsState {
    pub labels: Vec<String>,
    pub active_index: u32,
}

#[derive(Debug, Clone)]
pub struct OverlayState {
    pub open: bool,
    pub modal: bool,
    pub clear_under: bool,
    pub dismiss_on_escape: bool,
    pub restore_focus: Option<u32>,
}

impl Default for OverlayState {
    fn default() -> Self {
        Self {
            open: false,
            modal: false,
            clear_under: false,
            dismiss_on_escape: true,
            restore_focus: None,
        }
    }
}

// ============================================================================
// TextArea Editor State (ADR-T28)
// ============================================================================

#[derive(Debug, Clone)]
pub struct TextAreaEdit {
    pub cursor_row_before: u32,
    pub cursor_col_before: u32,
    pub selection_anchor_before: Option<(u32, u32)>,
    pub selection_focus_before: Option<(u32, u32)>,
    pub cursor_row_after: u32,
    pub cursor_col_after: u32,
    pub selection_anchor_after: Option<(u32, u32)>,
    pub selection_focus_after: Option<(u32, u32)>,
}

#[derive(Debug, Clone)]
pub struct TextAreaState {
    pub selection_anchor: Option<(u32, u32)>,
    pub selection_focus: Option<(u32, u32)>,
    pub undo_stack: VecDeque<TextAreaEdit>,
    pub redo_stack: VecDeque<TextAreaEdit>,
    pub history_limit: u32,
}

impl TextAreaState {
    /// Clear the current selection (anchor and focus).
    pub fn clear_selection(&mut self) {
        self.selection_anchor = None;
        self.selection_focus = None;
    }
}

impl Default for TextAreaState {
    fn default() -> Self {
        Self {
            selection_anchor: None,
            selection_focus: None,
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
            history_limit: 256,
        }
    }
}

// ============================================================================
// Transcript Types (ADR-T32, ADR-T33)
// ============================================================================

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptBlockKind {
    Message = 0,
    ToolCall = 1,
    ToolResult = 2,
    Reasoning = 3,
    Activity = 4,
    Divider = 5,
}

impl TranscriptBlockKind {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Message),
            1 => Some(Self::ToolCall),
            2 => Some(Self::ToolResult),
            3 => Some(Self::Reasoning),
            4 => Some(Self::Activity),
            5 => Some(Self::Divider),
            _ => None,
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowMode {
    Manual = 0,
    TailLocked = 1,
    TailWhileNearBottom = 2,
}

impl FollowMode {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Manual),
            1 => Some(Self::TailLocked),
            2 => Some(Self::TailWhileNearBottom),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ViewportAnchorKind {
    Tail,
    BlockStart { block_id: u64, row_offset: u32 },
    FocusedBlock { block_id: u64, row_offset: u32 },
}

#[derive(Debug, Clone)]
pub struct TranscriptBlock {
    pub id: u64,
    pub kind: TranscriptBlockKind,
    pub parent_id: Option<u64>,
    pub role: u8,
    pub buffer_handle: u32,
    pub view_handle: u32,
    pub content_format: ContentFormat,
    pub code_language: Option<String>,
    pub streaming: bool,
    pub collapsed: bool,
    pub hidden: bool,
    pub unread: bool,
    pub rendered_rows: u32,
    pub version: u64,
}

#[derive(Debug, Clone)]
pub struct TranscriptState {
    pub blocks: Vec<TranscriptBlock>,
    pub block_index: HashMap<u64, usize>,
    pub follow_mode: FollowMode,
    pub anchor_kind: ViewportAnchorKind,
    pub unread_anchor: Option<u64>,
    pub unread_count: u32,
    pub sticky_threshold_rows: u32,
    pub tail_attached: bool,
    pub viewport_rows: u32,
    /// Width of the viewport in columns, used for line-wrapping estimates.
    pub viewport_width: u32,
    /// Per-role foreground colors. Index = role (0=system, 1=user, 2=assistant,
    /// 3=tool, 4=reasoning). Value 0 means "inherit node default fg".
    /// Encoded as 0x01RRGGBB (RGB tag).
    pub role_colors: [u32; 5],
}

impl Default for TranscriptState {
    fn default() -> Self {
        Self {
            blocks: Vec::new(),
            block_index: HashMap::new(),
            follow_mode: FollowMode::TailWhileNearBottom,
            anchor_kind: ViewportAnchorKind::Tail,
            unread_anchor: None,
            unread_count: 0,
            sticky_threshold_rows: 2,
            tail_attached: true,
            viewport_rows: 0,
            viewport_width: 80,
            role_colors: [0; 5], // all inherit from node fg by default
        }
    }
}

// ============================================================================
// SplitPane Types (ADR-T35)
// ============================================================================

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SplitAxis {
    Horizontal = 0,
    Vertical = 1,
}

impl SplitAxis {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Horizontal),
            1 => Some(Self::Vertical),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SplitPaneState {
    pub axis: SplitAxis,
    pub primary_ratio_permille: u16,
    pub min_primary: u16,
    pub min_secondary: u16,
    pub resize_step: u16,
    pub resizable: bool,
}

impl Default for SplitPaneState {
    fn default() -> Self {
        Self {
            axis: SplitAxis::Horizontal,
            primary_ratio_permille: 500,
            min_primary: 0,
            min_secondary: 0,
            resize_step: 1,
            resizable: true,
        }
    }
}

// ============================================================================
// Native Text Substrate (ADR-T37, target state Epic M)
// ============================================================================

/// Soft-wrap mode for `TextView`.
///
/// Locked in `docs/spikes/CORE-M0-substrate-contract.md`.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WrapMode {
    None = 0,
    Char = 1,
    Word = 2,
}

impl WrapMode {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::None),
            1 => Some(Self::Char),
            2 => Some(Self::Word),
            _ => None,
        }
    }
}

/// Inclusive-exclusive byte range with style data attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StyleSpan {
    pub start: usize,
    pub end: usize,
    pub fg: u32,
    pub bg: u32,
    pub attrs: CellAttrs,
}

/// Inclusive-exclusive byte range describing the active selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionRange {
    pub start: usize,
    pub end: usize,
}

/// Inclusive-exclusive byte range marked with an arbitrary `kind` discriminant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighlightRange {
    pub start: usize,
    pub end: usize,
    pub kind: u8,
}

/// Inclusive-exclusive byte range that has changed since the buffer was created
/// or last had its dirty list consumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyRange {
    pub start: usize,
    pub old_end: usize,
    pub new_end: usize,
}

// ============================================================================
// Debug / Devtools Types (ADR-T34)
// ============================================================================

/// Maximum number of trace entries retained per trace kind.
pub const DEBUG_TRACE_MAX: usize = 256;

/// Overlay rendering flag bits.
pub mod overlay_flags {
    pub const BOUNDS: u32 = 0x01;
    pub const FOCUS: u32 = 0x02;
    pub const DIRTY: u32 = 0x04;
    pub const ANCHORS: u32 = 0x08;
    pub const PERF: u32 = 0x10;
}

/// Trace kind discriminants.
pub mod trace_kind {
    pub const EVENT: u8 = 0;
    pub const FOCUS: u8 = 1;
    pub const DIRTY: u8 = 2;
    pub const VIEWPORT: u8 = 3;
    pub const COUNT: usize = 4;
}

#[derive(Debug, Clone, Serialize)]
pub struct DebugTraceEntry {
    pub seq: u64,
    pub kind: u8,
    pub target: u32,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DebugFrameSnapshot {
    pub frame_id: u64,
    pub focused: u32,
    pub dirty_nodes: u32,
    pub diff_cells: u32,
    pub write_runs: u32,
    pub transcript_blocks: u32,
    pub transcript_unread: u32,
    pub tail_attached: bool,
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
    pub text_buffer_handle: Option<u32>,
    pub text_view_handle: Option<u32>,
    pub edit_buffer_handle: Option<u32>,
    pub children: Vec<u32>,
    pub parent: Option<u32>,
    pub visual_style: VisualStyle,
    pub dirty: bool,
    pub focusable: bool,
    pub visible: bool,
    pub scroll_x: i32,
    pub scroll_y: i32,
    pub show_scrollbar: bool,
    pub scrollbar_side: u8,  // 0=right, 1=left
    pub scrollbar_width: u8, // valid 1..=3
    pub render_offset: (f32, f32),
    pub z_index: i32,
    // Input widget state
    pub cursor_position: u32,
    pub max_length: u32,
    pub mask_char: u32,
    // TextArea widget state
    pub cursor_row: u32,
    pub cursor_col: u32,
    pub wrap_mode: u8,
    pub textarea_view_row: u32,
    pub textarea_view_col: u32,
    // Select widget state
    pub options: Vec<String>,
    pub selected_index: Option<u32>,
    // Accessibility fields (ADR-T23)
    pub role: Option<AccessibilityRole>,
    pub label: Option<String>,
    pub description: Option<String>,
    // TextArea editor state (ADR-T28)
    pub textarea_state: Option<TextAreaState>,
    // v3 widget state (ADR-T27)
    pub table_state: Option<TableState>,
    pub list_state: Option<ListState>,
    pub tabs_state: Option<TabsState>,
    pub overlay_state: Option<OverlayState>,
    // Transcript widget state (ADR-T32)
    pub transcript_state: Option<TranscriptState>,
    // SplitPane widget state (ADR-T35)
    pub split_pane_state: Option<SplitPaneState>,
}

impl TuiNode {
    pub fn new(node_type: NodeType, taffy_node: taffy::NodeId) -> Self {
        let focusable = matches!(
            node_type,
            NodeType::Input
                | NodeType::Select
                | NodeType::TextArea
                | NodeType::Table
                | NodeType::List
                | NodeType::Tabs
                | NodeType::Transcript
                | NodeType::SplitPane
        );
        Self {
            node_type,
            taffy_node,
            content: String::new(),
            content_format: ContentFormat::Plain,
            code_language: None,
            text_buffer_handle: None,
            text_view_handle: None,
            edit_buffer_handle: None,
            children: Vec::new(),
            parent: None,
            visual_style: VisualStyle::default(),
            dirty: true,
            focusable,
            visible: true,
            scroll_x: 0,
            scroll_y: 0,
            show_scrollbar: false,
            scrollbar_side: 0,
            scrollbar_width: 1,
            render_offset: (0.0, 0.0),
            z_index: 0,
            cursor_position: 0,
            max_length: 0,
            mask_char: 0,
            cursor_row: 0,
            cursor_col: 0,
            wrap_mode: 0,
            textarea_view_row: 0,
            textarea_view_col: 0,
            options: Vec::new(),
            selected_index: None,
            role: None,
            label: None,
            description: None,
            textarea_state: if node_type == NodeType::TextArea {
                Some(TextAreaState::default())
            } else {
                None
            },
            table_state: if node_type == NodeType::Table {
                Some(TableState::default())
            } else {
                None
            },
            list_state: if node_type == NodeType::List {
                Some(ListState::default())
            } else {
                None
            },
            tabs_state: if node_type == NodeType::Tabs {
                Some(TabsState::default())
            } else {
                None
            },
            overlay_state: if node_type == NodeType::Overlay {
                Some(OverlayState::default())
            } else {
                None
            },
            transcript_state: if node_type == NodeType::Transcript {
                Some(TranscriptState::default())
            } else {
                None
            },
            split_pane_state: if node_type == NodeType::SplitPane {
                Some(SplitPaneState::default())
            } else {
                None
            },
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
        assert_eq!(NodeType::from_u8(5), Some(NodeType::TextArea));
        assert_eq!(NodeType::from_u8(6), Some(NodeType::Table));
        assert_eq!(NodeType::from_u8(7), Some(NodeType::List));
        assert_eq!(NodeType::from_u8(8), Some(NodeType::Tabs));
        assert_eq!(NodeType::from_u8(9), Some(NodeType::Overlay));
        assert_eq!(NodeType::from_u8(10), Some(NodeType::Transcript));
        assert_eq!(NodeType::from_u8(11), Some(NodeType::SplitPane));
        assert_eq!(NodeType::from_u8(12), None);
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

    #[test]
    fn test_anim_prop_from_u8_v2_values() {
        assert_eq!(AnimProp::from_u8(0), Some(AnimProp::Opacity));
        assert_eq!(AnimProp::from_u8(4), Some(AnimProp::PositionX));
        assert_eq!(AnimProp::from_u8(5), Some(AnimProp::PositionY));
        assert_eq!(AnimProp::from_u8(6), None);
    }

    #[test]
    fn test_accessibility_role_from_u32() {
        assert_eq!(
            AccessibilityRole::from_u32(0),
            Some(AccessibilityRole::Button)
        );
        assert_eq!(
            AccessibilityRole::from_u32(1),
            Some(AccessibilityRole::Checkbox)
        );
        assert_eq!(
            AccessibilityRole::from_u32(2),
            Some(AccessibilityRole::Input)
        );
        assert_eq!(
            AccessibilityRole::from_u32(3),
            Some(AccessibilityRole::TextArea)
        );
        assert_eq!(
            AccessibilityRole::from_u32(4),
            Some(AccessibilityRole::List)
        );
        assert_eq!(
            AccessibilityRole::from_u32(5),
            Some(AccessibilityRole::ListItem)
        );
        assert_eq!(
            AccessibilityRole::from_u32(6),
            Some(AccessibilityRole::Heading)
        );
        assert_eq!(
            AccessibilityRole::from_u32(7),
            Some(AccessibilityRole::Region)
        );
        assert_eq!(
            AccessibilityRole::from_u32(8),
            Some(AccessibilityRole::Status)
        );
        assert_eq!(AccessibilityRole::from_u32(9), None);
    }

    #[test]
    fn test_tui_node_accessibility_defaults() {
        let mut taffy_tree: taffy::TaffyTree<()> = taffy::TaffyTree::new();
        let taffy_node = taffy_tree.new_leaf(taffy::Style::DEFAULT).unwrap();
        let node = TuiNode::new(NodeType::Box, taffy_node);
        assert_eq!(node.role, None);
        assert_eq!(node.label, None);
        assert_eq!(node.description, None);
    }

    #[test]
    fn test_accessibility_event_constructor() {
        let event = TuiEvent::accessibility(42, AccessibilityRole::Button as u32);
        assert_eq!(event.event_type, TuiEventType::Accessibility as u32);
        assert_eq!(event.target, 42);
        assert_eq!(event.data[0], 0); // Button = 0
    }

    #[test]
    fn test_easing_from_u8_v2_values() {
        assert_eq!(Easing::from_u8(0), Some(Easing::Linear));
        assert_eq!(Easing::from_u8(4), Some(Easing::CubicIn));
        assert_eq!(Easing::from_u8(5), Some(Easing::CubicOut));
        assert_eq!(Easing::from_u8(6), Some(Easing::Elastic));
        assert_eq!(Easing::from_u8(7), Some(Easing::Bounce));
        assert_eq!(Easing::from_u8(8), None);
    }
}
