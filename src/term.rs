//! Terminal setup and teardown.

use std::io;

use crossterm::{cursor, event, execute, terminal};

pub fn enter() -> io::Result<()> {
    terminal::enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, terminal::EnterAlternateScreen, event::EnableMouseCapture, cursor::SetCursorStyle::BlinkingBar)?;
    let _ = execute!(out, event::EnableBracketedPaste); // not supported everywhere
    Ok(())
}

pub fn leave() {
    let mut out = io::stdout();
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
