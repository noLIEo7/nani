//! System clipboard (arboard), falling back to OSC 52 + an internal buffer (e.g. over SSH).

use std::io::Write;

#[derive(Default)]
pub struct Clip {
    sys: Option<arboard::Clipboard>,
    tried: bool,
    internal: String,
    line_mode: bool,
}

impl Clip {
    // Connect lazily – keeps startup fast.
    fn sys(&mut self) -> Option<&mut arboard::Clipboard> {
        if !self.tried {
            self.tried = true;
            self.sys = arboard::Clipboard::new().ok();
        }
        self.sys.as_mut()
    }

    /// `line`: a whole line was copied (no selection) – it gets pasted above the current line.
    pub fn copy(&mut self, text: String, line: bool) {
        let ok = self.sys().is_some_and(|c| c.set_text(text.clone()).is_ok());
        if !ok {
            osc52(&text);
        }
        self.internal = text;
        self.line_mode = line;
    }

    pub fn paste(&mut self) -> Option<String> {
        if let Some(t) = self.sys().and_then(|c| c.get_text().ok()) {
            return Some(t);
        }
        (!self.internal.is_empty()).then(|| self.internal.clone())
    }

    pub fn is_line(&self, text: &str) -> bool {
        self.line_mode && text == self.internal
    }
}

fn osc52(text: &str) {
    let mut out = std::io::stdout();
    let _ = write!(out, "\x1b]52;c;{}\x07", base64(text.as_bytes()));
    let _ = out.flush();
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for ch in data.chunks(3) {
        let n = (ch[0] as u32) << 16
            | (*ch.get(1).unwrap_or(&0) as u32) << 8
            | *ch.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= ch.len() {
                s.push(T[(n >> (18 - 6 * i)) as usize & 63] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64() {
        assert_eq!(super::base64(b"Ma"), "TWE=");
        assert_eq!(super::base64(b"Man"), "TWFu");
        assert_eq!(super::base64(b"M"), "TQ==");
    }
}
