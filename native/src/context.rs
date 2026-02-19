//! TuiContext struct and global state accessor.
//!
//! The context owns all mutable state for the TUI system.
//! A single global instance is managed via `tui_init()` / `tui_shutdown()`.

use std::collections::HashMap;

use crate::terminal::TerminalBackend;
use crate::theme::Theme;
use crate::types::{Buffer, TuiEvent, TuiNode};

pub struct TuiContext {
    // Tree Module
    pub tree: taffy::TaffyTree<()>,
    pub nodes: HashMap<u32, TuiNode>,
    pub next_handle: u32,
    pub root: Option<u32>,

    // Event Module
    pub event_buffer: Vec<TuiEvent>,
    pub focused: Option<u32>,

    // Render Module
    pub front_buffer: Buffer,
    pub back_buffer: Buffer,
    pub backend: Box<dyn TerminalBackend>,

    // Text Module
    pub syntax_set: syntect::parsing::SyntaxSet,
    pub theme_set: syntect::highlighting::ThemeSet,

    // Theme Module
    pub themes: HashMap<u32, Theme>,
    pub theme_bindings: HashMap<u32, u32>, // node_handle -> theme_handle
    pub next_theme_handle: u32,

    // Diagnostics
    pub last_error: String,
    pub debug_mode: bool,
    pub perf_layout_us: u64,
    pub perf_render_us: u64,
    pub perf_diff_cells: u32,
}

impl TuiContext {
    pub fn new(backend: Box<dyn TerminalBackend>) -> Self {
        let (w, h) = backend.size();
        Self {
            tree: taffy::TaffyTree::new(),
            nodes: HashMap::new(),
            next_handle: 1, // Handle(0) is permanently invalid
            root: None,

            event_buffer: Vec::new(),
            focused: None,

            front_buffer: Buffer::new(w, h),
            back_buffer: Buffer::new(w, h),
            backend,

            syntax_set: syntect::parsing::SyntaxSet::load_defaults_newlines(),
            theme_set: syntect::highlighting::ThemeSet::load_defaults(),

            themes: {
                let mut t = HashMap::new();
                crate::theme::create_builtin_themes(&mut t);
                t
            },
            theme_bindings: HashMap::new(),
            next_theme_handle: crate::theme::FIRST_USER_THEME_HANDLE,

            last_error: String::new(),
            debug_mode: false,
            perf_layout_us: 0,
            perf_render_us: 0,
            perf_diff_cells: 0,
        }
    }

    /// Validate that a handle refers to an existing node.
    pub fn validate_handle(&self, handle: u32) -> Result<(), String> {
        if handle == 0 {
            return Err("Handle(0) is the invalid sentinel".to_string());
        }
        if !self.nodes.contains_key(&handle) {
            return Err(format!("Invalid handle: {handle}"));
        }
        Ok(())
    }

    pub fn debug_log(&self, msg: &str) {
        if self.debug_mode {
            eprintln!("[kraken-tui] {msg}");
        }
    }
}

// ============================================================================
// Global State
// ============================================================================

#[allow(static_mut_refs)]
static mut CONTEXT: Option<TuiContext> = None;

/// Get an immutable reference to the global context.
///
/// # Safety
/// This accesses a global `static mut`. The entire Native Core is single-threaded
/// per ADR-003, so this is safe within that invariant.
#[allow(static_mut_refs)]
pub fn context() -> Result<&'static TuiContext, String> {
    unsafe {
        CONTEXT
            .as_ref()
            .ok_or_else(|| "Context not initialized. Call tui_init() first.".to_string())
    }
}

/// Get a mutable reference to the global context.
///
/// # Safety
/// Same single-threaded invariant as `context()`.
#[allow(static_mut_refs)]
pub fn context_mut() -> Result<&'static mut TuiContext, String> {
    unsafe {
        CONTEXT
            .as_mut()
            .ok_or_else(|| "Context not initialized. Call tui_init() first.".to_string())
    }
}

/// Initialize the global context with the given backend.
#[allow(static_mut_refs)]
pub fn init_context(backend: Box<dyn TerminalBackend>) {
    unsafe {
        CONTEXT = Some(TuiContext::new(backend));
    }
}

/// Destroy the global context and return the backend for shutdown.
#[allow(static_mut_refs)]
pub fn destroy_context() -> Option<Box<dyn TerminalBackend>> {
    unsafe { CONTEXT.take().map(|ctx| ctx.backend) }
}

/// Store an error message in the global context (best-effort; ignores if no context).
///
/// Appends a null byte so the string can be safely returned as a C string
/// pointer via `tui_get_last_error()`. Without this, reading the pointer
/// as a C string would cause undefined behavior (buffer overread).
#[allow(static_mut_refs)]
pub fn set_last_error(msg: String) {
    unsafe {
        if let Some(ref mut ctx) = CONTEXT {
            let mut s = msg;
            s.push('\0');
            ctx.last_error = s;
        }
    }
}
