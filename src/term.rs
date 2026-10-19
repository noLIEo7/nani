//! Terminal setup and teardown.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::event::KeyboardEnhancementFlags;
use crossterm::{cursor, event, execute, terminal};

/// The kitty keyboard protocol is on, so `leave` has to switch it off again.
static ENHANCED_KEYS: AtomicBool = AtomicBool::new(false);

pub fn enter() -> io::Result<()> {
    terminal::enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, terminal::EnterAlternateScreen, event::EnableMouseCapture, cursor::SetCursorStyle::BlinkingBar)?;
    let _ = execute!(out, event::EnableBracketedPaste); // not supported everywhere
    // Kitty keyboard protocol where supported: Ctrl+Shift+Z no longer arrives as Ctrl+Z,
    // and Ctrl+/ gets through on every layout. After the alternate screen: each screen
    // keeps its own setting.
    let flags = event::PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES);
    if terminal::supports_keyboard_enhancement().unwrap_or(false) && execute!(out, flags).is_ok() {
        ENHANCED_KEYS.store(true, Ordering::Relaxed);
    }
    Ok(())
}

pub fn leave() {
    let mut out = io::stdout();
    if ENHANCED_KEYS.swap(false, Ordering::Relaxed) {
        let _ = execute!(out, event::PopKeyboardEnhancementFlags);
    }
    let _ = execute!(out, event::DisableBracketedPaste);
    let _ = execute!(
        out,
        event::DisableMouseCapture,
        cursor::SetCursorStyle::DefaultUserShape,
        terminal::LeaveAlternateScreen,
        cursor::Show
    );
    let _ = terminal::disable_raw_mode();
}
