//! Editor state, input handling and core editing.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::SystemTime;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use unicode_width::UnicodeWidthChar;

use crate::buffer::{Buffer, Kind, Pos};
use crate::clipboard::Clip;
use crate::syntax::{Lang, State};

pub const TAB_WIDTH: usize = 4;

/// Screen columns taken by `c` at column `x` (tabs go to the next tab stop).
pub fn advance(x: usize, c: char) -> usize {
    if c == '\t' {
        TAB_WIDTH - x % TAB_WIDTH
    } else {
        c.width().unwrap_or(1)
    }
}

/// Width of `s` when it starts at column 0 (of a screen row).
pub fn width(s: &[char]) -> usize {
    s.iter().fold(0, |x, &c| x + advance(x, c))
}

pub fn str_width(s: &str) -> usize {
    s.chars().fold(0, |x, c| x + advance(x, c))
}

/// Start indices of the screen rows of a wrapped line (word wrap).
pub fn wrap_segments(line: &[char], width: usize) -> Vec<usize> {
    let mut segs = vec![0];
    let (mut start, mut w, mut brk, mut i) = (0, 0, None, 0);
    while i < line.len() {
        let cw = advance(w, line[i]);
        if w + cw > width && i > start {
            let s = match brk {
                Some(b) if b > start && b <= i => b,
                _ => i,
            };
            segs.push(s);
            start = s;
            brk = None;
            w = self::width(&line[start..i]);
            continue;
        }
        w += cw;
        if line[i] == ' ' {
            brk = Some(i + 1);
        }
        i += 1;
    }
    segs
}

pub fn seg_of(segs: &[usize], col: usize) -> usize {
    segs.partition_point(|&s| s <= col) - 1
}

/// Column within a segment that corresponds to screen position `x`.
pub fn col_at_x(line: &[char], start: usize, end: usize, last: bool, x: usize) -> usize {
    let mut acc = 0;
    for (c, &ch) in line.iter().enumerate().take(end).skip(start) {
        let cw = advance(acc, ch);
        if acc + cw > x {
            return c;
        }
        acc += cw;
    }
    if last {
        end
    } else {
        end.saturating_sub(1).max(start)
    }
}

fn class(c: char) -> u8 {
    if c.is_whitespace() {
        0
    } else if c.is_alphanumeric() || c == '_' {
        1
    } else {
        2
    }
}

/// Parses `LINE` or `LINE:COL` (1-based).
pub fn parse_goto(s: &str) -> Option<(usize, usize)> {
    let (l, c) = match s.split_once([':', ',']) {
        Some((l, c)) => (l, Some(c)),
        None => (s, None),
    };
    let l: usize = l.trim().parse().ok()?;
    let c: usize = match c {
        Some(c) => c.trim().parse().ok()?,
        None => 1,
    };
    Some((l.max(1), c.max(1)))
}

pub fn expand_tilde(s: &str) -> PathBuf {
    if s == "~" || s.starts_with("~/") || s.starts_with("~\\") {
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            return PathBuf::from(home).join(s[1..].trim_start_matches(['/', '\\']));
        }
    }
    PathBuf::from(s)
}

/// Completes a path as far as it is unambiguous.
fn complete_path(s: &str) -> Option<String> {
    let split = s.rfind(|c| c == '/' || (cfg!(windows) && c == '\\')).map_or(0, |i| i + 1);
    let (dir, prefix) = s.split_at(split);
    let dir_path = if dir.is_empty() { PathBuf::from(".") } else { expand_tilde(dir) };
    let mut names: Vec<(String, bool)> = std::fs::read_dir(dir_path)
        .ok()?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let visible = prefix.starts_with('.') || !name.starts_with('.');
            (name.starts_with(prefix) && visible).then(|| (name, e.path().is_dir()))
        })
        .collect();
    names.sort();
    let (first, is_dir) = names.first()?.clone();
    let mut common: Vec<char> = first.chars().collect();
    for (n, _) in &names[1..] {
        let k = common.iter().zip(n.chars()).take_while(|(a, b)| **a == *b).count();
        common.truncate(k);
    }
    let mut out = format!("{dir}{}", common.iter().collect::<String>());
    if names.len() == 1 && is_dir {
        out.push('/');
    }
    Some(out)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Indent {
    Tabs,
    Spaces(usize),
}

impl Indent {
    pub fn unit(self) -> String {
        match self {
            Indent::Tabs => "\t".into(),
            Indent::Spaces(n) => " ".repeat(n),
        }
    }

    pub fn describe(self) -> String {
        match self {
            Indent::Tabs => "Tabs".into(),
            Indent::Spaces(n) => format!("Spaces: {n}"),
        }
    }
}

/// Guesses the indentation style from the first lines of a file.
pub fn detect_indent(buf: &Buffer, lang: Lang) -> (Indent, bool) {
    let (mut tabs, mut spaced) = (0usize, 0usize);
    let mut steps = [0usize; 9];
    let mut prev = 0usize;
    let mut line = Vec::new();
    for l in 0..buf.lines().min(2000) {
        buf.line_into(l, &mut line);
        if line.iter().all(|c| c.is_whitespace()) {
            continue;
        }
        if line[0] == '\t' {
            tabs += 1;
            continue;
        }
        let n = line.iter().take_while(|&&c| c == ' ').count();
        if n > 0 {
            spaced += 1;
        }
        let d = n.abs_diff(prev);
        if (2..=8).contains(&d) {
            steps[d] += 1;
        }
        prev = n;
    }
    if tabs > spaced {
        return (Indent::Tabs, true);
    }
    let best = (2..=8).map(|n| (n, steps[n])).filter(|&(_, c)| c > 0).max_by_key(|&(n, c)| (c, std::cmp::Reverse(n)));
    if let (true, Some((n, _))) = (spaced > 0, best) {
        return (Indent::Spaces(n), true);
    }
    (lang.default_indent().map_or(Indent::Tabs, Indent::Spaces), false)
}

#[derive(Default)]
pub struct Prompt {
    pub text: Vec<char>,
    pub cur: usize,
}

pub enum PromptKey {
    Submit,
    Cancel,
    Edited,
    Other,
}

impl Prompt {
    pub fn key(&mut self, k: &KeyEvent) -> PromptKey {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let alt = k.modifiers.contains(KeyModifiers::ALT);
        match k.code {
            KeyCode::Enter => PromptKey::Submit,
            KeyCode::Esc => PromptKey::Cancel,
            KeyCode::Char('c' | 'q') if ctrl && !alt => PromptKey::Cancel,
            KeyCode::Char(c) if ctrl == alt => {
                self.text.insert(self.cur, c);
                self.cur += 1;
                PromptKey::Edited
            }
            KeyCode::Backspace if self.cur > 0 => {
                self.cur -= 1;
                self.text.remove(self.cur);
                PromptKey::Edited
            }
            KeyCode::Delete if self.cur < self.text.len() => {
                self.text.remove(self.cur);
                PromptKey::Edited
            }
            KeyCode::Left => {
                self.cur = self.cur.saturating_sub(1);
                PromptKey::Other
            }
            KeyCode::Right => {
                self.cur = (self.cur + 1).min(self.text.len());
                PromptKey::Other
            }
            KeyCode::Home => {
                self.cur = 0;
                PromptKey::Other
            }
            KeyCode::End => {
                self.cur = self.text.len();
                PromptKey::Other
            }
            _ => PromptKey::Other,
        }
    }

    pub fn insert_str(&mut self, s: &str) {
        for c in s.chars().take_while(|&c| c != '\n' && c != '\r') {
            self.text.insert(self.cur, c);
            self.cur += 1;
        }
    }

    pub fn string(&self) -> String {
        self.text.iter().collect()
    }

    fn complete_path(&mut self) {
        if let Some(done) = complete_path(&self.string()) {
            self.text = done.chars().collect();
            self.cur = self.text.len();
        }
    }
}

/// What happens when a yes/no question is answered with yes.
pub enum Action {
    Write(PathBuf),
    Sudo(PathBuf),
    /// Save although invalid UTF-8 bytes were replaced on load.
    SaveLossy,
}

pub enum Mode {
    Normal,
    Help,
    SaveAs(Prompt),
    Confirm { question: String, action: Action },
    Find { p: Prompt, origin: Pos, anchor: Option<Pos>, found: bool },
    ReplaceFind(Prompt),
    ReplaceWith { query: Vec<char>, p: Prompt },
    Replacing { query: Vec<char>, with: String, start: usize, last: usize, wrapped: bool, count: usize },
    GoTo(Prompt),
    Command(Prompt),
    InsertFile(Prompt),
}

/// A cached line: (line, chars, wrap segments).
type CachedLine = (usize, Rc<Vec<char>>, Option<Rc<Vec<usize>>>);

/// Key (buffer version, text width, wrap) and entries.
#[derive(Default)]
pub struct LineCache {
    key: (u64, usize, bool),
    entries: Vec<CachedLine>,
}

#[derive(Clone, Copy)]
pub struct RowInfo {
    pub line: usize,
    pub start: usize,
    pub end: usize,
    pub last: bool,
}

pub struct Editor {
    pub(crate) buf: Buffer,
    pub(crate) path: Option<PathBuf>,
    pub(crate) lang: Lang,
    pub(crate) indent: Indent,
    pub(crate) indent_detected: bool,
    pub(crate) readonly: bool,
    /// The file was not valid UTF-8 – saving would replace the invalid bytes.
    pub(crate) lossy: bool,
    pub(crate) cursor: Pos,
    pub(crate) anchor: Option<Pos>,
    pub(crate) want_x: Option<usize>,
    pub(crate) wrap: bool,
    pub(crate) numbers: bool,
    pub(crate) top_line: usize,
    pub(crate) top_seg: usize,
    pub(crate) left: usize,
    pub(crate) mode: Mode,
    pub(crate) msg: Option<(String, bool)>,
    pub(crate) quit_armed: bool,
    pub quit: bool,
    pub(crate) clip: Clip,
    pub(crate) w: u16,
    pub(crate) h: u16,
    pub(crate) row_map: Vec<Option<RowInfo>>,
    pub(crate) wrap_btn: (u16, u16),
    pub(crate) dragging: bool,
    /// Highlighter state at the start of each line (valid for `0..hl.len()`).
    pub(crate) hl: Vec<State>,
    /// Modification time and size of the file when it was loaded or saved.
    pub(crate) stamp: Option<(SystemTime, u64)>,
    pub(crate) disk_warned: bool,
    pub(crate) cache: RefCell<LineCache>,
}

impl Editor {
    pub fn new(text: &str, path: Option<PathBuf>, msg: Option<String>) -> Self {
        let buf = Buffer::new(text);
        let first: String = buf.rope().line(0).chars().take(200).collect();
        let lang = Lang::detect(path.as_deref(), &first);
        let (indent, indent_detected) = detect_indent(&buf, lang);
        Editor {
            buf,
            path,
            lang,
            indent,
            indent_detected,
            readonly: false,
            lossy: false,
            cursor: Pos::default(),
            anchor: None,
            want_x: None,
            wrap: true,
            numbers: true,
            top_line: 0,
            top_seg: 0,
            left: 0,
            mode: Mode::Normal,
            msg: msg.map(|m| (m, false)),
            quit_armed: false,
            quit: false,
            clip: Clip::default(),
            w: 80,
            h: 24,
            row_map: Vec::new(),
            wrap_btn: (0, 0),
            dragging: false,
            hl: vec![State::Normal],
            stamp: None,
            disk_warned: false,
            cache: RefCell::default(),
        }
    }

    pub fn resize(&mut self, w: u16, h: u16) {
        self.w = w.max(1);
        self.h = h.max(1);
        self.scroll_to_cursor();
    }

    /// Moves the cursor to a 1-based line/column and centers it on screen.
    pub fn goto(&mut self, line: usize, col: usize) {
        self.cursor = self.clamp(Pos::new(line.saturating_sub(1), col.saturating_sub(1)));
        self.anchor = None;
        self.want_x = None;
        self.top_line = self.cursor.line.saturating_sub(self.text_rows() / 2);
        self.top_seg = 0;
        self.scroll_to_cursor();
    }

    // ---------- Geometry ----------

    pub(crate) fn msg_visible(&self) -> bool {
        self.msg.is_some() || !matches!(self.mode, Mode::Normal | Mode::Help)
    }

    pub(crate) fn text_rows(&self) -> usize {
        let reserved = 1 + self.msg_visible() as usize;
        (self.h as usize).saturating_sub(reserved).max(1)
    }

    pub(crate) fn gutter(&self) -> usize {
        if self.numbers {
            self.buf.lines().to_string().len() + 2
        } else {
            0
        }
    }

    /// Width of the text area (one column on the right is kept free for the cursor).
    pub(crate) fn text_width(&self) -> usize {
        (self.w as usize).saturating_sub(self.gutter() + 1).max(1)
    }

    /// A line's chars and (if `with_segs`) its wrap segments, from a small cache that is
    /// reset on every edit – so very long lines are not copied and re-wrapped over and over.
    pub(crate) fn cached(&self, l: usize, with_segs: bool) -> (Rc<Vec<char>>, Option<Rc<Vec<usize>>>) {
        const SIZE: usize = 16;
        let key = (self.buf.version(), self.text_width(), self.wrap);
        let mut cache = self.cache.borrow_mut();
        if cache.key != key {
            cache.key = key;
            cache.entries.clear();
        }
        let i = match cache.entries.iter().position(|e| e.0 == l) {
            Some(i) => i,
            None => {
                if cache.entries.len() >= SIZE {
                    cache.entries.remove(0);
                }
                cache.entries.push((l, Rc::new(self.buf.line(l)), None));
                cache.entries.len() - 1
            }
        };
        if with_segs && cache.entries[i].2.is_none() {
            let segs = if self.wrap { wrap_segments(&cache.entries[i].1, key.1) } else { vec![0] };
            cache.entries[i].2 = Some(Rc::new(segs));
        }
        let e = &cache.entries[i];
        (e.1.clone(), e.2.clone())
    }

    pub(crate) fn line(&self, l: usize) -> Rc<Vec<char>> {
        self.cached(l, false).0
    }

    pub(crate) fn segments(&self, l: usize) -> Rc<Vec<usize>> {
        self.cached(l, true).1.unwrap_or_default()
    }

    pub(crate) fn selection(&self) -> Option<(Pos, Pos)> {
        let a = self.anchor?;
        (a != self.cursor).then(|| (a.min(self.cursor), a.max(self.cursor)))
    }

    /// Line range of a multi-line selection (last line only if something on it is selected).
    pub(crate) fn selected_lines(&self) -> Option<(usize, usize)> {
        let (a, b) = self.selection()?;
        (a.line != b.line).then(|| (a.line, if b.col == 0 { b.line - 1 } else { b.line }))
    }

    pub(crate) fn line_range(&self) -> (usize, usize) {
        self.selected_lines().unwrap_or((self.cursor.line, self.cursor.line))
    }

    pub(crate) fn clamp(&self, p: Pos) -> Pos {
        let line = p.line.min(self.buf.lines() - 1);
        Pos::new(line, p.col.min(self.buf.len(line)))
    }

    pub(crate) fn info(&mut self, s: impl Into<String>) {
        self.msg = Some((s.into(), false));
    }

    pub(crate) fn error(&mut self, s: impl Into<String>) {
        self.msg = Some((s.into(), true));
    }

    pub(crate) fn editable(&mut self) -> bool {
        if self.readonly {
            self.error("Read-only mode");
        }
        !self.readonly
    }

    // ---------- Scrolling ----------

    fn clamp_top(&mut self) {
        self.top_line = self.top_line.min(self.buf.lines() - 1);
        self.top_seg = self.top_seg.min(self.segments(self.top_line).len() - 1);
    }

    /// Scrolls by `n` screen rows, jumping over whole lines at once.
    pub(crate) fn scroll_down(&mut self, mut n: usize) {
        while n > 0 {
            let rows = self.segments(self.top_line).len();
            let room = rows - 1 - self.top_seg;
            if n <= room {
                self.top_seg += n;
                return;
            }
            if self.top_line + 1 >= self.buf.lines() {
                self.top_seg = rows - 1;
                return;
            }
            n -= room + 1;
            self.top_line += 1;
            self.top_seg = 0;
        }
    }

    pub(crate) fn scroll_up(&mut self, mut n: usize) {
        while n > 0 {
            if n <= self.top_seg {
                self.top_seg -= n;
                return;
            }
            if self.top_line == 0 {
                self.top_seg = 0;
                return;
            }
            n -= self.top_seg + 1;
            self.top_line -= 1;
            self.top_seg = self.segments(self.top_line).len() - 1;
        }
    }

    pub(crate) fn scroll_to_cursor(&mut self) {
        self.cursor = self.clamp(self.cursor);
        // safety net: a selection end must never point past the text
        self.anchor = self.anchor.map(|a| self.clamp(a));
        self.clamp_top();
        let rows = self.text_rows();
        let segs = self.segments(self.cursor.line);
        let cs = seg_of(&segs, self.cursor.col);
        if (self.cursor.line, cs) < (self.top_line, self.top_seg) {
            self.top_line = self.cursor.line;
            self.top_seg = cs;
        } else if self.cursor.line > self.top_line + rows {
            self.top_line = self.cursor.line;
            self.top_seg = cs;
            self.scroll_up(rows - 1);
        } else {
            let mut dist = cs as isize - self.top_seg as isize;
            for l in self.top_line..self.cursor.line {
                dist += self.segments(l).len() as isize;
            }
            if dist >= rows as isize {
                self.scroll_down(dist as usize + 1 - rows);
            }
        }
        if self.wrap {
            self.left = 0;
        } else {
            let tw = self.text_width();
            let x = width(&self.line(self.cursor.line)[..self.cursor.col]);
            if x < self.left {
                self.left = x;
            } else if x >= self.left + tw {
                self.left = x + 1 - tw;
            }
        }
    }

    // ---------- Events ----------

    pub fn handle(&mut self, ev: Event) {
        match ev {
            Event::Key(k) if k.kind != KeyEventKind::Release => {
                self.on_key(k);
                self.scroll_to_cursor();
            }
            Event::Paste(s) => {
                self.on_paste(&s);
                self.scroll_to_cursor();
            }
            Event::Mouse(m) => self.on_mouse(m),
            Event::Resize(w, h) => self.resize(w, h),
            _ => {}
        }
    }

    fn on_paste(&mut self, s: &str) {
        match &mut self.mode {
            Mode::Normal => {
                self.msg = None;
                self.paste_text(s, false);
            }
            Mode::SaveAs(p)
            | Mode::ReplaceFind(p)
            | Mode::ReplaceWith { p, .. }
            | Mode::GoTo(p)
            | Mode::Command(p)
            | Mode::InsertFile(p) => p.insert_str(s),
            Mode::Find { p, .. } => {
                p.insert_str(s);
                self.search_update();
            }
            Mode::Help | Mode::Confirm { .. } | Mode::Replacing { .. } => {}
        }
    }

    fn on_key(&mut self, k: KeyEvent) {
        match self.mode {
            Mode::Normal => {}
            Mode::Help => {
                self.mode = Mode::Normal;
                return;
            }
            _ => return self.on_prompt_key(k),
        }
        let armed = std::mem::take(&mut self.quit_armed);
        self.msg = None;
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let alt = k.modifiers.contains(KeyModifiers::ALT);
        let shift = k.modifiers.contains(KeyModifiers::SHIFT);
        match k.code {
            // ctrl == alt: plain key or AltGr combination (Windows)
            KeyCode::Char(c) if ctrl == alt => self.type_char(c),
            KeyCode::Char(c) if ctrl => self.command(c.to_ascii_lowercase(), shift, armed),
            KeyCode::Char(_) => {}
            KeyCode::Up if alt => self.move_lines(false),
            KeyCode::Down if alt => self.move_lines(true),
            KeyCode::Enter => self.newline(),
            KeyCode::Tab => self.indent_lines(),
            KeyCode::BackTab => self.unindent(),
            KeyCode::Backspace if ctrl || alt => self.delete_word(false),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete if ctrl => self.delete_word(true),
            KeyCode::Delete => self.delete_forward(),
            KeyCode::Esc => self.anchor = None,
            KeyCode::F(1) => self.mode = Mode::Help,
            code => self.movement(code, ctrl, shift),
        }
    }

    fn command(&mut self, c: char, shift: bool, armed: bool) {
        match c {
            'q' => {
                if self.buf.dirty() && !armed {
                    self.quit_armed = true;
                    self.error("Unsaved changes! Ctrl+Q again = discard, Ctrl+S = save");
                } else {
                    self.quit = true;
                }
            }
            's' => self.save(),
            'c' => self.copy(),
            'x' => self.cut(),
            'v' => self.paste(),
            'z' if shift => self.redo(),
            'z' => self.undo(),
            'y' => self.redo(),
            'a' => {
                self.buf.close_group();
                self.anchor = Some(Pos::default());
                self.cursor = self.buf.doc_end();
            }
            'f' => {
                self.buf.close_group();
                self.mode = Mode::Find { p: Prompt::default(), origin: self.cursor, anchor: self.anchor, found: true };
            }
            'r' => {
                if self.editable() {
                    self.mode = Mode::ReplaceFind(Prompt::default());
                }
            }
            'g' => self.mode = Mode::GoTo(Prompt::default()),
            'd' => self.duplicate(),
            // Ctrl+/ arrives as Ctrl+7 or Ctrl+_ in most terminals
            '/' | '7' | '_' => self.toggle_comment(),
            'b' => self.jump_bracket(),
            't' => self.format_json(),
            'e' => {
                if self.editable() {
                    self.mode = Mode::Command(Prompt::default());
                }
            }
            'p' => {
                if self.editable() {
                    self.mode = Mode::InsertFile(Prompt::default());
                }
            }
            'w' => self.toggle_wrap(),
            'n' => self.toggle_numbers(),
            'k' => self.mode = Mode::Help,
            // Ctrl+Backspace arrives as Ctrl+H in many terminals
            'h' => self.delete_word(false),
            _ => {}
        }
    }

    fn on_prompt_key(&mut self, k: KeyEvent) {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && k.code == KeyCode::Char('v') {
            if let Some(t) = self.clip.paste() {
                self.on_paste(&t);
            }
            return;
        }
        let tab = k.code == KeyCode::Tab;
        match std::mem::replace(&mut self.mode, Mode::Normal) {
            Mode::Normal | Mode::Help => {}
            Mode::Confirm { question, action } => match k.code {
                KeyCode::Char('y' | 'Y') => self.run_action(action),
                KeyCode::Char('n' | 'N') | KeyCode::Esc => self.info("Cancelled"),
                KeyCode::Char('c' | 'q') if ctrl => self.info("Cancelled"),
                _ => self.mode = Mode::Confirm { question, action },
            },
            Mode::Replacing { query, with, start, last, wrapped, count } => {
                self.mode = Mode::Replacing { query, with, start, last, wrapped, count };
                self.replacing_key(k);
            }
            Mode::Find { mut p, origin, anchor, found } => match p.key(&k) {
                PromptKey::Submit => {}
                PromptKey::Cancel => {
                    self.cursor = origin;
                    self.anchor = anchor;
                }
                PromptKey::Edited => {
                    self.mode = Mode::Find { p, origin, anchor, found };
                    self.search_update();
                }
                PromptKey::Other => {
                    self.mode = Mode::Find { p, origin, anchor, found };
                    match k.code {
                        KeyCode::Down => self.search_step(true),
                        KeyCode::Char('f') if ctrl => self.search_step(true),
                        KeyCode::Up => self.search_step(false),
                        _ => {}
                    }
                }
            },
            Mode::SaveAs(mut p) if tab => {
                p.complete_path();
                self.mode = Mode::SaveAs(p);
            }
            Mode::InsertFile(mut p) if tab => {
                p.complete_path();
                self.mode = Mode::InsertFile(p);
            }
            Mode::SaveAs(p) => self.prompt_step(&k, p, Mode::SaveAs, |ed, p| ed.save_as(p.string())),
            Mode::InsertFile(p) => self.prompt_step(&k, p, Mode::InsertFile, |ed, p| ed.insert_file(&p.string())),
            Mode::GoTo(p) => self.prompt_step(&k, p, Mode::GoTo, |ed, p| ed.goto_input(&p.string())),
            Mode::Command(p) => self.prompt_step(&k, p, Mode::Command, |ed, p| ed.run_command(&p.string())),
            Mode::ReplaceFind(p) => self.prompt_step(&k, p, Mode::ReplaceFind, |ed, p| {
                if p.text.is_empty() {
                    ed.info("Cancelled");
                } else {
                    ed.mode = Mode::ReplaceWith { query: p.text, p: Prompt::default() };
                }
            }),
            Mode::ReplaceWith { query, mut p } => match p.key(&k) {
                PromptKey::Submit => self.start_replacing(query, p.string()),
                PromptKey::Cancel => self.info("Cancelled"),
                _ => self.mode = Mode::ReplaceWith { query, p },
            },
        }
    }

    /// Feeds a key to a simple one-line prompt.
    fn prompt_step(&mut self, k: &KeyEvent, mut p: Prompt, back: fn(Prompt) -> Mode, submit: fn(&mut Self, Prompt)) {
        match p.key(k) {
            PromptKey::Submit => submit(self, p),
            PromptKey::Cancel => self.info("Cancelled"),
            _ => self.mode = back(p),
        }
    }

    fn on_mouse(&mut self, m: MouseEvent) {
        let (x, y) = (m.column, m.row);
        if matches!(self.mode, Mode::Help) {
            if matches!(m.kind, MouseEventKind::Down(_)) {
                self.mode = Mode::Normal;
            }
            return;
        }
        match m.kind {
            MouseEventKind::ScrollDown => self.scroll_down(3),
            MouseEventKind::ScrollUp => self.scroll_up(3),
            _ if !matches!(self.mode, Mode::Normal) => {}
            MouseEventKind::Down(MouseButton::Left) => {
                if y == self.h - 1 {
                    if x >= self.wrap_btn.0 && x < self.wrap_btn.1 {
                        self.toggle_wrap();
                    }
                    return;
                }
                if let Some(p) = self.pos_at(x, y) {
                    self.buf.close_group();
                    self.quit_armed = false;
                    if m.modifiers.contains(KeyModifiers::SHIFT) {
                        self.anchor.get_or_insert(self.cursor);
                    } else {
                        self.anchor = Some(p);
                    }
                    self.cursor = p;
                    self.want_x = None;
                    self.dragging = true;
                    self.scroll_to_cursor();
                }
            }
            MouseEventKind::Drag(MouseButton::Left) if self.dragging => {
                let rows = self.text_rows() as u16;
                let y = if y >= rows {
                    self.scroll_down(1);
                    rows - 1
                } else if y == 0 {
                    self.scroll_up(1);
                    0
                } else {
                    y
                };
                if let Some(p) = self.pos_at(x, y) {
                    self.cursor = p;
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.dragging = false;
                if self.anchor == Some(self.cursor) {
                    self.anchor = None;
                }
            }
            _ => {}
        }
    }

    /// Text position under a screen coordinate (as of the last render).
    fn pos_at(&self, x: u16, y: u16) -> Option<Pos> {
        if y as usize >= self.row_map.len() {
            return None;
        }
        let Some(r) = self.row_map[y as usize] else {
            return Some(self.buf.doc_end());
        };
        let left = if self.wrap { 0 } else { self.left };
        let xx = (x as usize).saturating_sub(self.gutter()) + left;
        let line = self.line(r.line);
        Some(Pos::new(r.line, col_at_x(&line, r.start, r.end, r.last, xx)))
    }

    // ---------- Movement ----------

    pub(crate) fn set_cursor(&mut self, p: Pos, select: bool) {
        if select {
            self.anchor.get_or_insert(self.cursor);
        } else {
            self.anchor = None;
        }
        self.cursor = p;
    }

    fn left(&self, p: Pos) -> Pos {
        if p.col > 0 {
            Pos::new(p.line, p.col - 1)
        } else if p.line > 0 {
            Pos::new(p.line - 1, self.buf.len(p.line - 1))
        } else {
            p
        }
    }

    fn right(&self, p: Pos) -> Pos {
        if p.col < self.buf.len(p.line) {
            Pos::new(p.line, p.col + 1)
        } else if p.line + 1 < self.buf.lines() {
            Pos::new(p.line + 1, 0)
        } else {
            p
        }
    }

    fn word_left(&self, p: Pos) -> Pos {
        if p.col == 0 {
            return self.left(p);
        }
        let line = self.line(p.line);
        let mut c = p.col;
        while c > 0 && class(line[c - 1]) == 0 {
            c -= 1;
        }
        if c > 0 {
            let k = class(line[c - 1]);
            while c > 0 && class(line[c - 1]) == k {
                c -= 1;
            }
        }
        Pos::new(p.line, c)
    }

    fn word_right(&self, p: Pos) -> Pos {
        let line = self.line(p.line);
        if p.col >= line.len() {
            return self.right(p);
        }
        let mut c = p.col;
        while c < line.len() && class(line[c]) == 0 {
            c += 1;
        }
        if c < line.len() {
            let k = class(line[c]);
            while c < line.len() && class(line[c]) == k {
                c += 1;
            }
        }
        Pos::new(p.line, c)
    }

    fn cursor_x(&self) -> usize {
        let line = self.line(self.cursor.line);
        let segs = self.segments(self.cursor.line);
        let s = segs[seg_of(&segs, self.cursor.col)];
        width(&line[s..self.cursor.col])
    }

    fn pos_in_seg(&self, l: usize, segs: &[usize], s: usize, x: usize) -> Pos {
        let line = self.line(l);
        let last = s + 1 == segs.len();
        let end = if last { line.len() } else { segs[s + 1] };
        Pos::new(l, col_at_x(&line, segs[s], end, last, x))
    }

    fn vertical(&self, p: Pos, down: bool, x: usize) -> Pos {
        let segs = self.segments(p.line);
        let s = seg_of(&segs, p.col);
        if down {
            if s + 1 < segs.len() {
                self.pos_in_seg(p.line, &segs, s + 1, x)
            } else if p.line + 1 < self.buf.lines() {
                let sg = self.segments(p.line + 1);
                self.pos_in_seg(p.line + 1, &sg, 0, x)
            } else {
                Pos::new(p.line, self.buf.len(p.line))
            }
        } else if s > 0 {
            self.pos_in_seg(p.line, &segs, s - 1, x)
        } else if p.line > 0 {
            let sg = self.segments(p.line - 1);
            self.pos_in_seg(p.line - 1, &sg, sg.len() - 1, x)
        } else {
            Pos::new(0, 0)
        }
    }

    fn movement(&mut self, code: KeyCode, ctrl: bool, shift: bool) {
        let c = self.cursor;
        let sel = self.selection();
        let mut keep_x = false;
        let target = match code {
            KeyCode::Left => match sel {
                Some((a, _)) if !shift => a,
                _ if ctrl => self.word_left(c),
                _ => self.left(c),
            },
            KeyCode::Right => match sel {
                Some((_, b)) if !shift => b,
                _ if ctrl => self.word_right(c),
                _ => self.right(c),
            },
            KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown => {
                keep_x = true;
                let n = match code {
                    KeyCode::Up | KeyCode::Down => 1,
                    _ => self.text_rows().saturating_sub(1).max(1),
                };
                let down = matches!(code, KeyCode::Down | KeyCode::PageDown);
                let x = self.want_x.unwrap_or_else(|| self.cursor_x());
                self.want_x = Some(x);
                (0..n).fold(c, |p, _| self.vertical(p, down, x))
            }
            KeyCode::Home if ctrl => Pos::default(),
            KeyCode::Home => {
                let first = self.line(c.line).iter().take_while(|ch| ch.is_whitespace()).count();
                Pos::new(c.line, if c.col == first { 0 } else { first })
            }
            KeyCode::End if ctrl => self.buf.doc_end(),
            KeyCode::End => Pos::new(c.line, self.buf.len(c.line)),
            _ => return,
        };
        if !keep_x {
            self.want_x = None;
        }
        self.buf.close_group();
        self.set_cursor(target, shift);
    }

    // ---------- Editing ----------

    pub(crate) fn insert(&mut self, text: &str, kind: Option<Kind>) {
        let sel = self.selection();
        self.buf.begin(self.cursor, if sel.is_some() { None } else { kind });
        if let Some((a, b)) = sel {
            self.buf.delete(a, b);
            self.cursor = a;
        }
        self.anchor = None;
        self.cursor = self.buf.insert(self.cursor, text);
        self.buf.end(self.cursor);
        self.want_x = None;
    }

    pub(crate) fn remove(&mut self, a: Pos, b: Pos, kind: Option<Kind>) {
        self.buf.begin(self.cursor, kind);
        self.buf.delete(a, b);
        self.cursor = a;
        self.anchor = None;
        self.buf.end(a);
        self.want_x = None;
    }

    fn type_char(&mut self, c: char) {
        if !self.editable() {
            return;
        }
        if c == ' ' {
            self.buf.close_group(); // undo word by word
        }
        let mut b = [0u8; 4];
        self.insert(c.encode_utf8(&mut b), Some(Kind::Type));
    }

    /// Enter: keeps the indentation and indents after an opening bracket (or `:` in Python/YAML).
    fn newline(&mut self) {
        if !self.editable() {
            return;
        }
        self.buf.close_group();
        let sel = self.selection();
        self.buf.begin(self.cursor, None);
        if let Some((a, b)) = sel {
            self.buf.delete(a, b);
            self.cursor = a;
            self.anchor = None;
        }
        let line = self.line(self.cursor.line);
        let c = self.cursor.col;
        let base: String = line[..c].iter().take_while(|&&ch| ch == ' ' || ch == '\t').collect();
        let before = line[..c].iter().rev().find(|ch| !ch.is_whitespace()).copied();
        let after = line.get(c).copied();
        let colon = matches!(self.lang, Lang::Python | Lang::Yaml) && before == Some(':');
        let opens = matches!(before, Some('{' | '[' | '(')) || colon;
        let mut text = format!("\n{base}");
        if opens {
            text += &self.indent.unit();
        }
        self.cursor = self.buf.insert(self.cursor, &text);
        if matches!((before, after), (Some('{'), Some('}')) | (Some('['), Some(']')) | (Some('('), Some(')'))) {
            let keep = self.cursor;
            self.buf.insert(self.cursor, &format!("\n{base}"));
            self.cursor = keep;
        }
        self.buf.end(self.cursor);
        self.buf.close_group();
        self.want_x = None;
    }

    fn backspace(&mut self) {
        if !self.editable() {
            return;
        }
        if let Some((a, b)) = self.selection() {
            return self.remove(a, b, None);
        }
        let c = self.cursor;
        let start = if c.col > 0 {
            let line = self.line(c.line);
            match self.indent {
                // indentation made of spaces: delete back to the previous indent stop
                Indent::Spaces(n) if line[..c.col].iter().all(|&ch| ch == ' ') => {
                    Pos::new(c.line, c.col - ((c.col - 1) % n + 1))
                }
                _ => Pos::new(c.line, c.col - 1),
            }
        } else if c.line > 0 {
            Pos::new(c.line - 1, self.buf.len(c.line - 1))
        } else {
            return;
        };
        self.remove(start, c, Some(Kind::Delete));
    }

    fn delete_forward(&mut self) {
        if !self.editable() {
            return;
        }
        if let Some((a, b)) = self.selection() {
            return self.remove(a, b, None);
        }
        let c = self.cursor;
        let end = self.right(c);
        if end != c {
            self.remove(c, end, Some(Kind::Delete));
        }
    }

    fn delete_word(&mut self, forward: bool) {
        if !self.editable() {
            return;
        }
        if let Some((a, b)) = self.selection() {
            return self.remove(a, b, None);
        }
        let c = self.cursor;
        let (a, b) = if forward { (c, self.word_right(c)) } else { (self.word_left(c), c) };
        if a != b {
            self.buf.close_group();
            self.remove(a, b, None);
        }
    }

    fn indent_lines(&mut self) {
        if !self.editable() {
            return;
        }
        let unit = self.indent.unit();
        let Some((first, last)) = self.selected_lines() else {
            let text = match self.indent {
                Indent::Tabs => unit,
                Indent::Spaces(n) => {
                    let x = width(&self.line(self.cursor.line)[..self.cursor.col]);
                    " ".repeat(n - x % n)
                }
            };
            return self.insert(&text, None);
        };
        let n = unit.chars().count();
        self.buf.begin(self.cursor, None);
        for l in first..=last {
            self.buf.insert(Pos::new(l, 0), &unit);
        }
        let shift = |p: Pos| if (first..=last).contains(&p.line) { Pos::new(p.line, p.col + n) } else { p };
        self.cursor = shift(self.cursor);
        self.anchor = self.anchor.map(shift);
        self.buf.end(self.cursor);
    }

    fn unindent(&mut self) {
        if !self.editable() {
            return;
        }
        let (first, last) = self.line_range();
        let max = match self.indent {
            Indent::Tabs => TAB_WIDTH,
            Indent::Spaces(n) => n,
        };
        self.buf.begin(self.cursor, None);
        for l in first..=last {
            let line = self.line(l);
            let n = if line.first() == Some(&'\t') {
                1
            } else {
                line.iter().take(max).take_while(|&&c| c == ' ').count()
            };
            if n > 0 {
                self.buf.delete(Pos::new(l, 0), Pos::new(l, n));
                let shift = |p: Pos| if p.line == l { Pos::new(l, p.col.saturating_sub(n)) } else { p };
                self.cursor = shift(self.cursor);
                self.anchor = self.anchor.map(shift);
            }
        }
        self.buf.end(self.cursor);
    }

    fn undo(&mut self) {
        if !self.editable() {
            return;
        }
        match self.buf.undo() {
            Some(p) => {
                self.cursor = self.clamp(p);
                self.anchor = None;
                self.want_x = None;
            }
            None => self.info("Nothing to undo"),
        }
    }

    fn redo(&mut self) {
        if !self.editable() {
            return;
        }
        match self.buf.redo() {
            Some(p) => {
                self.cursor = self.clamp(p);
                self.anchor = None;
                self.want_x = None;
            }
            None => self.info("Nothing to redo"),
        }
    }

    // ---------- Clipboard ----------

    fn copy(&mut self) {
        if let Some((a, b)) = self.selection() {
            self.clip.copy(self.buf.text(a, b), false);
            self.info(format!("Copied {} characters", self.buf.char_count(a, b)));
        } else {
            self.clip.copy(self.buf.line_string(self.cursor.line) + "\n", true);
            self.info("Line copied");
        }
    }

    fn cut(&mut self) {
        if !self.editable() {
            return;
        }
        if let Some((a, b)) = self.selection() {
            self.clip.copy(self.buf.text(a, b), false);
            self.remove(a, b, None);
            self.info("Cut");
            return;
        }
        let l = self.cursor.line;
        self.clip.copy(self.buf.line_string(l) + "\n", true);
        let n = self.buf.lines();
        let (a, b) = if l + 1 < n {
            (Pos::new(l, 0), Pos::new(l + 1, 0))
        } else if l > 0 {
            (Pos::new(l - 1, self.buf.len(l - 1)), Pos::new(l, self.buf.len(l)))
        } else {
            (Pos::new(0, 0), Pos::new(0, self.buf.len(0)))
        };
        self.remove(a, b, None);
        self.cursor = Pos::new(l.min(self.buf.lines() - 1), 0);
        self.info("Line cut");
    }

    fn paste(&mut self) {
        match self.clip.paste() {
            Some(t) => {
                let line = self.clip.is_line(&t);
                self.paste_text(&t, line);
            }
            None => self.error("Clipboard is empty"),
        }
    }

    pub(crate) fn paste_text(&mut self, t: &str, line_mode: bool) {
        if !self.editable() {
            return;
        }
        let text = t.replace("\r\n", "\n").replace('\r', "\n");
        if text.is_empty() {
            return;
        }
        self.buf.close_group();
        if line_mode && self.selection().is_none() {
            // a copied whole line goes above the current line
            let col = self.cursor.col;
            self.cursor.col = 0;
            self.insert(&text, None);
            self.cursor = self.clamp(Pos::new(self.cursor.line, col));
        } else {
            self.insert(&text, None);
        }
        self.buf.close_group();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn wrap_words() {
        assert_eq!(wrap_segments(&chars("hallo welt foo"), 8), vec![0, 6]);
        assert_eq!(wrap_segments(&chars("hallo welt foo"), 7), vec![0, 6, 11]);
        assert_eq!(wrap_segments(&chars("abcdefghij"), 4), vec![0, 4, 8]);
        assert_eq!(wrap_segments(&chars(""), 4), vec![0]);
        assert_eq!(wrap_segments(&chars("日本語"), 4), vec![0, 2]);
        assert_eq!(wrap_segments(&chars("ab\tcd"), 5), vec![0, 4]);
    }

    #[test]
    fn tab_stops() {
        assert_eq!(width(&chars("a\tb")), 5);
        assert_eq!(width(&chars("abcd\tb")), 9);
        assert_eq!(str_width("\t\t"), 8);
        // x = 2 is inside the tab (columns 1..4) → the tab itself
        assert_eq!(col_at_x(&chars("a\tb"), 0, 3, true, 2), 1);
        assert_eq!(col_at_x(&chars("a\tb"), 0, 3, true, 4), 2);
    }

    #[test]
    fn indent_detection() {
        let d = |s: &str, lang| detect_indent(&Buffer::new(s), lang);
        assert_eq!(d("a\n  b\n    c\n  d", Lang::Plain), (Indent::Spaces(2), true));
        assert_eq!(d("a\n    b\n        c", Lang::Plain), (Indent::Spaces(4), true));
        assert_eq!(d("a:\n\tb\n\tc", Lang::Plain), (Indent::Tabs, true));
        assert_eq!(d("x", Lang::Makefile), (Indent::Tabs, false));
        assert_eq!(d("x", Lang::Yaml), (Indent::Spaces(2), false));
        assert_eq!(d("x", Lang::Rust), (Indent::Spaces(4), false));
    }

    #[test]
    fn goto_parsing() {
        assert_eq!(parse_goto("12"), Some((12, 1)));
        assert_eq!(parse_goto("12:5"), Some((12, 5)));
        assert_eq!(parse_goto("0"), Some((1, 1)));
        assert_eq!(parse_goto("x"), None);
    }
}
