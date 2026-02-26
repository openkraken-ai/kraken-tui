//! TuiContext struct and global state accessor.
//!
//! The context owns all mutable state for the TUI system.
//! A single global instance is managed via `tui_init()` / `tui_shutdown()`.

use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::{OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard};
#[cfg(not(test))]
use std::thread::ThreadId;
use std::time::Instant;

use crate::animation::Animation;
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

    // Animation Module (v1)
    pub animations: Vec<Animation>,
    pub animation_chains: HashMap<u32, u32>, // after_anim_id → next_anim_id
    pub next_anim_handle: u32,
    pub last_render_time: Option<Instant>,

    // Diagnostics
    pub last_error: String,
    pub debug_mode: bool,
    pub perf_layout_us: u64,
    pub perf_render_us: u64,
    pub perf_diff_cells: u32,
}

// SAFETY: ADR-T16 preserves Kraken TUI's single-threaded execution model.
// The lock is used for aliasing safety at the FFI boundary, not to introduce
// concurrent access. We never intentionally share mutable context access across
// threads in production code paths.
unsafe impl Send for TuiContext {}
unsafe impl Sync for TuiContext {}

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

            animations: Vec::new(),
            animation_chains: HashMap::new(),
            next_anim_handle: 1,
            last_render_time: None,

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

static CONTEXT: OnceLock<RwLock<Option<TuiContext>>> = OnceLock::new();
#[cfg(not(test))]
static OWNER_THREAD: OnceLock<RwLock<Option<ThreadId>>> = OnceLock::new();

fn context_lock() -> &'static RwLock<Option<TuiContext>> {
    CONTEXT.get_or_init(|| RwLock::new(None))
}

#[cfg(not(test))]
fn owner_thread_lock() -> &'static RwLock<Option<ThreadId>> {
    OWNER_THREAD.get_or_init(|| RwLock::new(None))
}

fn lock_poisoned(name: &str, detail: impl std::fmt::Display) -> String {
    format!("{name} lock poisoned after panic: {detail}")
}

fn ensure_thread_affinity() -> Result<(), String> {
    #[cfg(test)]
    {
        return Ok(());
    }

    #[cfg(not(test))]
    {
        let current = std::thread::current().id();
        let owner = owner_thread_lock()
            .read()
            .map_err(|e| lock_poisoned("owner_thread", e))?;
        if let Some(owner_id) = *owner {
            if owner_id != current {
                return Err("Context access from non-owner thread is unsupported".to_string());
            }
        }
        Ok(())
    }
}

#[cfg(not(test))]
fn bind_owner_thread_current() -> Result<(), String> {
    let current = std::thread::current().id();
    let mut owner = owner_thread_lock()
        .write()
        .map_err(|e| lock_poisoned("owner_thread", e))?;
    if let Some(owner_id) = *owner {
        if owner_id != current {
            return Err("Context access from non-owner thread is unsupported".to_string());
        }
    }
    *owner = Some(current);
    Ok(())
}

#[cfg(test)]
fn bind_owner_thread_current() -> Result<(), String> {
    Ok(())
}

#[cfg(not(test))]
fn clear_owner_thread() -> Result<(), String> {
    let mut owner = owner_thread_lock()
        .write()
        .map_err(|e| lock_poisoned("owner_thread", e))?;
    *owner = None;
    Ok(())
}

#[cfg(test)]
fn clear_owner_thread() -> Result<(), String> {
    Ok(())
}

pub struct ContextReadGuard<'a> {
    guard: RwLockReadGuard<'a, Option<TuiContext>>,
}

impl Deref for ContextReadGuard<'_> {
    type Target = TuiContext;

    fn deref(&self) -> &Self::Target {
        self.guard
            .as_ref()
            .expect("ContextReadGuard is only constructed for initialized context")
    }
}

pub struct ContextWriteGuard<'a> {
    guard: RwLockWriteGuard<'a, Option<TuiContext>>,
}

impl Deref for ContextWriteGuard<'_> {
    type Target = TuiContext;

    fn deref(&self) -> &Self::Target {
        self.guard
            .as_ref()
            .expect("ContextWriteGuard is only constructed for initialized context")
    }
}

impl DerefMut for ContextWriteGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.guard
            .as_mut()
            .expect("ContextWriteGuard is only constructed for initialized context")
    }
}

/// Acquire a read lock for the global context.
pub fn context_read() -> Result<ContextReadGuard<'static>, String> {
    ensure_thread_affinity()?;
    let guard = context_lock()
        .read()
        .map_err(|e| lock_poisoned("context", e))?;
    if guard.is_none() {
        return Err("Context not initialized. Call tui_init() first.".to_string());
    }
    Ok(ContextReadGuard { guard })
}

/// Acquire a write lock for the global context.
pub fn context_write() -> Result<ContextWriteGuard<'static>, String> {
    ensure_thread_affinity()?;
    let guard = context_lock()
        .write()
        .map_err(|e| lock_poisoned("context", e))?;
    if guard.is_none() {
        return Err("Context not initialized. Call tui_init() first.".to_string());
    }
    Ok(ContextWriteGuard { guard })
}

/// Initialize the global context with the given backend.
pub fn init_context(backend: Box<dyn TerminalBackend>) -> Result<(), String> {
    ensure_thread_affinity()?;
    bind_owner_thread_current()?;

    let mut guard = context_lock()
        .write()
        .map_err(|e| lock_poisoned("context", e))?;
    if guard.is_some() {
        return Err("Context already initialized. Call tui_shutdown() first.".to_string());
    }
    *guard = Some(TuiContext::new(backend));
    Ok(())
}

/// Check whether a context is currently initialized.
pub fn is_context_initialized() -> Result<bool, String> {
    ensure_thread_affinity()?;
    let guard = context_lock()
        .read()
        .map_err(|e| lock_poisoned("context", e))?;
    Ok(guard.is_some())
}

/// Destroy the global context and return the backend for shutdown.
pub fn destroy_context() -> Result<Option<Box<dyn TerminalBackend>>, String> {
    ensure_thread_affinity()?;
    let mut guard = context_lock()
        .write()
        .map_err(|e| lock_poisoned("context", e))?;
    let backend = guard.take().map(|ctx| ctx.backend);
    drop(guard);
    clear_owner_thread()?;
    Ok(backend)
}

/// Store an error message in the global context (best-effort; ignores if no context).
pub fn set_last_error(msg: String) {
    if ensure_thread_affinity().is_err() {
        return;
    }
    if let Ok(mut guard) = context_lock().write() {
        if let Some(ctx) = guard.as_mut() {
            ctx.last_error = msg;
        }
    }
}

/// Clear the context-bound error message.
pub fn clear_last_error() {
    if ensure_thread_affinity().is_err() {
        return;
    }
    if let Ok(mut guard) = context_lock().write() {
        if let Some(ctx) = guard.as_mut() {
            ctx.last_error.clear();
        }
    }
}

/// Snapshot the last error into owned memory.
pub fn get_last_error_snapshot() -> Option<String> {
    if ensure_thread_affinity().is_err() {
        return None;
    }
    if let Ok(guard) = context_lock().read() {
        if let Some(ctx) = guard.as_ref() {
            if !ctx.last_error.is_empty() {
                return Some(ctx.last_error.clone());
            }
        }
    }

    None
}
