//! Text buffer (a rope, fully in RAM) with undo/redo.
//!
//! Only `\n` counts as a line break. The final newline of a file is not part of the
//! buffer – it is stripped on load and added back on save.

use std::cell::Cell;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;

use ropey::Rope;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

impl Pos {
    pub fn new(line: usize, col: usize) -> Self {
        Pos { line, col }
    }
}

/// Kind of edit – consecutive edits of the same kind are merged into one undo step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Type,
    Delete,
}

enum Edit {
    Insert { at: Pos, end: Pos, text: String },
    Delete { at: Pos, text: String },
}

struct Group {
    id: u64,
    edits: Vec<Edit>,
    before: Pos,
    after: Pos,
}

pub struct Buffer {
    rope: Rope,
    undo: Vec<Group>,
    redo: Vec<Group>,
    open: Option<Kind>,
    next_id: u64,
    saved_id: u64,
    /// Bumped on every text change.
    version: u64,
    /// Length and content hash of the last saved (or loaded) state.
    saved: (usize, u64),
    /// Cached result of the content comparison: (version, dirty).
    dirty_cache: Cell<(u64, bool)>,
    /// Smallest line changed since the last `take_touched()`.
    touched: usize,
}

fn end_of(at: Pos, text: &str) -> Pos {
    match text.rfind('\n') {
        None => Pos::new(at.line, at.col + text.chars().count()),
        Some(i) => Pos::new(at.line + text.matches('\n').count(), text[i + 1..].chars().count()),
    }
}

impl Buffer {
    pub fn new(text: &str) -> Self {
        let mut b = Buffer {
            rope: Rope::from_str(text),
            undo: Vec::new(),
            redo: Vec::new(),
            open: None,
            next_id: 0,
            saved_id: 0,
            version: 0,
            saved: (0, 0),
            dirty_cache: Cell::new((0, false)),
            touched: usize::MAX,
        };
        b.saved = b.fingerprint();
        b
    }

    pub fn rope(&self) -> &Rope {
        &self.rope
    }

    fn fingerprint(&self) -> (usize, u64) {
        let mut h = DefaultHasher::new();
        for chunk in self.rope.chunks() {
            h.write(chunk.as_bytes());
        }
        (self.rope.len_chars(), h.finish())
    }

    /// Changes with every edit (used to invalidate caches).
    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn lines(&self) -> usize {
        self.rope.len_lines()
    }

    /// Length of a line in chars, without the line break.
    pub fn len(&self, l: usize) -> usize {
        let s = self.rope.line(l);
        let n = s.len_chars();
        if n > 0 && s.char(n - 1) == '\n' {
            n - 1
        } else {
            n
        }
    }

    pub fn line(&self, l: usize) -> Vec<char> {
        let mut v = Vec::new();
        self.line_into(l, &mut v);
        v
    }

    pub fn line_into(&self, l: usize, out: &mut Vec<char>) {
        out.clear();
        out.extend(self.rope.line(l).chars().take(self.len(l)));
    }

    pub fn line_string(&self, l: usize) -> String {
        self.text(Pos::new(l, 0), Pos::new(l, self.len(l)))
    }

    pub fn idx(&self, p: Pos) -> usize {
        self.rope.line_to_char(p.line) + p.col
    }

    pub fn pos(&self, i: usize) -> Pos {
        let l = self.rope.char_to_line(i);
        Pos::new(l, i - self.rope.line_to_char(l))
    }

    pub fn doc_end(&self) -> Pos {
        let l = self.lines() - 1;
        Pos::new(l, self.len(l))
    }

    pub fn text(&self, a: Pos, b: Pos) -> String {
        self.rope.slice(self.idx(a)..self.idx(b)).to_string()
    }

    pub fn char_count(&self, a: Pos, b: Pos) -> usize {
        self.idx(b) - self.idx(a)
    }

    /// Returns (and resets) the smallest line changed since the last call.
    pub fn take_touched(&mut self) -> Option<usize> {
        let t = std::mem::replace(&mut self.touched, usize::MAX);
        (t != usize::MAX).then_some(t)
    }

    /// True if the text differs from the last saved (or loaded) state –
    /// typing something and deleting it again does not count as a change.
    pub fn dirty(&self) -> bool {
        if self.undo.last().map_or(0, |g| g.id) == self.saved_id {
            return false;
        }
        if self.rope.len_chars() != self.saved.0 {
            return true;
        }
        let (version, dirty) = self.dirty_cache.get();
        if version == self.version {
            return dirty;
        }
        let dirty = self.fingerprint() != self.saved;
        self.dirty_cache.set((self.version, dirty));
        dirty
    }

    pub fn mark_saved(&mut self) {
        self.open = None;
        self.saved_id = self.undo.last().map_or(0, |g| g.id);
        self.saved = self.fingerprint();
        self.dirty_cache.set((self.version, false));
    }

    /// Starts an undo group (or continues the open one if `kind` stays the same).
    pub fn begin(&mut self, cursor: Pos, kind: Option<Kind>) {
        let merge = kind.is_some() && kind == self.open && !self.undo.is_empty();
        if !merge {
            self.next_id += 1;
            self.undo.push(Group { id: self.next_id, edits: Vec::new(), before: cursor, after: cursor });
        }
        self.open = kind;
    }

    pub fn end(&mut self, cursor: Pos) {
        match self.undo.last_mut() {
            Some(g) if g.edits.is_empty() => {
                self.undo.pop();
                self.open = None;
            }
            Some(g) => g.after = cursor,
            None => {}
        }
    }

    pub fn close_group(&mut self) {
        self.open = None;
    }

    pub fn insert(&mut self, at: Pos, text: &str) -> Pos {
        let end = self.raw_insert(at, text);
        if text.is_empty() {
            return end;
        }
        self.redo.clear();
        let g = self.undo.last_mut().expect("begin() before insert()");
        if let Some(Edit::Insert { end: e, text: t, .. }) = g.edits.last_mut() {
            if *e == at {
                t.push_str(text);
                *e = end;
                return end;
            }
        }
        g.edits.push(Edit::Insert { at, end, text: text.to_string() });
        end
    }

    pub fn delete(&mut self, a: Pos, b: Pos) {
        if a >= b {
            return;
        }
        let text = self.raw_delete(a, b);
        self.redo.clear();
        let g = self.undo.last_mut().expect("begin() before delete()");
        if let Some(Edit::Delete { at, text: t }) = g.edits.last_mut() {
            if *at == b {
                // run of backspaces
                *t = text + t;
                *at = a;
                return;
            }
            if *at == a {
                // run of forward deletes
                t.push_str(&text);
                return;
            }
        }
        g.edits.push(Edit::Delete { at: a, text });
    }

    pub fn undo(&mut self) -> Option<Pos> {
        self.open = None;
        let g = self.undo.pop()?;
        for e in g.edits.iter().rev() {
            match e {
                Edit::Insert { at, end, .. } => {
                    self.raw_delete(*at, *end);
                }
                Edit::Delete { at, text } => {
                    self.raw_insert(*at, text);
                }
            }
        }
        let p = g.before;
        self.redo.push(g);
        Some(p)
    }

    pub fn redo(&mut self) -> Option<Pos> {
        self.open = None;
        let g = self.redo.pop()?;
        for e in &g.edits {
            match e {
                Edit::Insert { at, text, .. } => {
                    self.raw_insert(*at, text);
                }
                Edit::Delete { at, text } => {
                    self.raw_delete(*at, end_of(*at, text));
                }
            }
        }
        let p = g.after;
        self.undo.push(g);
        Some(p)
    }

    fn raw_insert(&mut self, at: Pos, text: &str) -> Pos {
        self.version += 1;
        self.touched = self.touched.min(at.line);
        let i = self.idx(at);
        self.rope.insert(i, text);
        self.pos(i + text.chars().count())
    }

    fn raw_delete(&mut self, a: Pos, b: Pos) -> String {
        self.version += 1;
        self.touched = self.touched.min(a.line);
        let (i, j) = (self.idx(a), self.idx(b));
        let s = self.rope.slice(i..j).to_string();
        self.rope.remove(i..j);
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(b: &Buffer) -> String {
        b.rope().to_string()
    }

    #[test]
    fn insert_delete_multiline() {
        let mut b = Buffer::new("hallo welt");
        b.begin(Pos::default(), None);
        let end = b.insert(Pos::new(0, 5), "\nneue\nzeile");
        assert_eq!(end, Pos::new(2, 5));
        assert_eq!(all(&b), "hallo\nneue\nzeile welt");
        assert_eq!(b.lines(), 3);
        assert_eq!(b.len(1), 4);
        b.delete(Pos::new(0, 2), Pos::new(2, 2));
        b.end(Pos::new(0, 2));
        assert_eq!(all(&b), "haile welt");
        assert!(b.dirty());
        b.undo();
        assert_eq!(all(&b), "hallo welt");
        assert!(!b.dirty());
        b.redo();
        assert_eq!(all(&b), "haile welt");
    }

    #[test]
    fn typing_merges_and_save_marker() {
        let mut b = Buffer::new("");
        let mut c = Pos::default();
        for ch in ["a", "b", "c"] {
            b.begin(c, Some(Kind::Type));
            c = b.insert(c, ch);
            b.end(c);
        }
        b.mark_saved();
        assert!(!b.dirty());
        for _ in 0..2 {
            b.begin(c, Some(Kind::Delete));
            let a = Pos::new(0, c.col - 1);
            b.delete(a, c);
            c = a;
            b.end(c);
        }
        assert_eq!(all(&b), "a");
        assert!(b.dirty());
        b.undo();
        assert_eq!(all(&b), "abc");
        assert!(!b.dirty());
        b.undo();
        assert_eq!(all(&b), "");
        assert!(b.undo().is_none());
    }

    #[test]
    fn retyped_or_deleted_text_is_not_dirty() {
        let mut b = Buffer::new("");
        b.begin(Pos::default(), Some(Kind::Type));
        let end = b.insert(Pos::default(), "hi");
        b.end(end);
        assert!(b.dirty());
        b.begin(end, Some(Kind::Delete));
        b.delete(Pos::default(), end);
        b.end(Pos::default());
        assert!(!b.dirty());
        b.mark_saved();
        b.undo();
        assert!(b.dirty());
    }

    #[test]
    fn positions() {
        let b = Buffer::new("abc\nde\nf");
        assert_eq!(b.char_count(Pos::new(0, 1), Pos::new(2, 1)), 7);
        assert_eq!(b.text(Pos::new(0, 1), Pos::new(2, 1)), "bc\nde\nf");
        assert_eq!(b.pos(5), Pos::new(1, 1));
        assert_eq!(b.doc_end(), Pos::new(2, 1));
        assert_eq!(b.line(1), vec!['d', 'e']);
    }
}
