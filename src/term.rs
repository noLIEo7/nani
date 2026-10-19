//! Terminal setup and teardown.

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::event::KeyboardEnhancementFlags;
use crossterm::{cursor, event, execute, terminal};

/// The kitty keyboard protocol is on, so `leave` has to switch it off again.
static ENHANCED_KEYS: AtomicBool = AtomicBool::new(false);
/// We switched on grapheme cluster widths (mode 2027), so `leave` switches them off again.
static CLUSTER_MODE: AtomicBool = AtomicBool::new(false);

pub fn enter() -> io::Result<()> {
    terminal::enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, terminal::EnterAlternateScreen, event::EnableMouseCapture, cursor::SetCursorStyle::BlinkingBar)?;
    let _ = execute!(out, event::EnableBracketedPaste); // not supported everywhere
    // Before crossterm reads anything from the terminal: the reply is read here.
    crate::editor::set_cluster_widths(cluster_widths(&mut out));
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
    if CLUSTER_MODE.swap(false, Ordering::Relaxed) {
        let _ = out.write_all(b"\x1b[?2027l").and_then(|()| out.flush());
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

/// Whether the terminal draws an emoji sequence like 👨‍👩‍👧 in the width of the whole cluster
/// (mode 2027), switching that on where it's supported but off. Most terminals add up the
/// widths of the code points instead.
#[cfg(unix)]
fn cluster_widths(out: &mut impl Write) -> bool {
    match query_mode_2027() {
        // set, or permanently set
        Some(1 | 3) => true,
        // supported but reset: switch it on
        Some(2) => {
            let on = out.write_all(b"\x1b[?2027h").and_then(|()| out.flush()).is_ok();
            CLUSTER_MODE.store(on, Ordering::Relaxed);
            on
        }
        _ => false,
    }
}

/// Windows Terminal (1.22 and newer) measures grapheme clusters; the old console doesn't.
#[cfg(not(unix))]
fn cluster_widths(_out: &mut impl Write) -> bool {
    std::env::var_os("WT_SESSION").is_some()
}

/// The terminal's answer to "is mode 2027 set?" (DECRQM): 0 unknown mode, 1 set, 2 reset,
/// 3 permanently set, 4 permanently reset; `None` without an answer. Device attributes are
/// asked for right after, which every terminal answers, so the wait ends with that answer.
#[cfg(unix)]
fn query_mode_2027() -> Option<u8> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    use std::time::{Duration, Instant};

    // a description of its own, so the non-blocking flag doesn't affect crossterm's
    let mut tty =
        std::fs::OpenOptions::new().read(true).write(true).custom_flags(libc::O_NONBLOCK).open("/dev/tty").ok()?;
    tty.write_all(b"\x1b[?2027$p\x1b[c").ok()?;
    let start = Instant::now();
    let (mut reply, mut chunk) = (Vec::new(), [0u8; 256]);
    while start.elapsed() < Duration::from_millis(500) && !has_device_attributes(&reply) {
        match tty.read(&mut chunk) {
            Ok(n) if n > 0 => reply.extend_from_slice(&chunk[..n]),
            _ => std::thread::sleep(Duration::from_millis(5)),
        }
    }
    parse_mode_2027(&reply)
}

/// The reply contains the primary device attributes, `ESC [ ? … c`.
#[cfg(unix)]
fn has_device_attributes(reply: &[u8]) -> bool {
    reply.windows(3).enumerate().any(|(i, w)| {
        w == b"\x1b[?" && reply[i + 3..].iter().find(|b| !(b.is_ascii_digit() || **b == b';')) == Some(&b'c')
    })
}

/// The state in a mode report for 2027, `ESC [ ? 2027 ; Ps $ y`.
#[cfg(unix)]
fn parse_mode_2027(reply: &[u8]) -> Option<u8> {
    const PREFIX: &[u8] = b"\x1b[?2027;";
    let at = reply.windows(PREFIX.len()).position(|w| w == PREFIX)? + PREFIX.len();
    match reply.get(at..at + 3)? {
        [d @ b'0'..=b'4', b'$', b'y'] => Some(d - b'0'),
        _ => None,
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn mode_reports() {
        assert_eq!(parse_mode_2027(b"\x1b[?2027;3$y\x1b[?62;22c"), Some(3));
        assert_eq!(parse_mode_2027(b"x\x1b[?2027;0$y"), Some(0));
        assert_eq!(parse_mode_2027(b"\x1b[?62;22c"), None);
        assert_eq!(parse_mode_2027(b"\x1b[?2027;9$y"), None);
        assert!(has_device_attributes(b"\x1b[?2027;2$y\x1b[?62;1;4c"));
        assert!(has_device_attributes(b"\x1b[?6c"));
        assert!(!has_device_attributes(b"\x1b[?2027;2$y"));
        assert!(!has_device_attributes(b"\x1b[?62;1"));
    }
}
