//! Editor commands: find/replace, go to line, line operations, comments, brackets,
//! JSON formatting, shell commands, inserting files, saving and watching the file on disk.

use std::io;
use std::path::PathBuf;

use crate::buffer::Pos;
use crate::editor::{expand_tilde, parse_goto, Action, Editor, Mode, Prompt};
use crate::fileio;
use crate::syntax::{Lang, State};

fn smart_case(q: &[char]) -> bool {
    !q.iter().any(|c| c.is_uppercase())
}

fn matches_at(line: &[char], c: usize, q: &[char], ci: bool) -> bool {
    c + q.len() <= line.len()
        && q.iter().zip(&line[c..]).all(|(&x, &y)| x == y || (ci && x.to_lowercase().eq(y.to_lowercase())))
}

impl Editor {
    // ---------- Find ----------

    /// Next (or previous) match of `q`, wrapping around the end of the document.
    pub(crate) fn find(&self, q: &[char], from: Pos, forward: bool) -> Option<Pos> {
        if q.is_empty() {
            return None;
        }
        let ci = smart_case(q);
        let n = self.buf.lines();
        let mut line = Vec::new();
        for k in 0..=n {
            let l = if forward { (from.line + k) % n } else { (from.line + n - k % n) % n };
            self.buf.line_into(l, &mut line);
            let len = line.len();
            let hit = |c: &usize| matches_at(&line, *c, q, ci);
            let found = match (forward, k) {
                (true, 0) => (from.col..=len).find(hit),
                (true, k) if k == n => (0..from.col.min(len + 1)).find(hit),
                (true, _) => (0..=len).find(hit),
                (false, 0) => (0..from.col.min(len + 1)).rev().find(hit),
                (false, k) if k == n => (from.col..=len).rev().find(hit),
                (false, _) => (0..=len).rev().find(hit),
            };
            if let Some(c) = found {
                return Some(Pos::new(l, c));
            }
        }
        None
    }

    fn select_match(&mut self, p: Pos, len: usize) {
        self.anchor = Some(p);
        self.cursor = Pos::new(p.line, p.col + len);
    }

    fn find_query(&self) -> Vec<char> {
        match &self.mode {
            Mode::Find { p, .. } => p.text.clone(),
            _ => Vec::new(),
        }
    }

    pub(crate) fn search_update(&mut self) {
        let Mode::Find { origin, .. } = self.mode else { return };
        let q = self.find_query();
        let hit = self.find(&q, origin, true);
        match hit {
            Some(p) => self.select_match(p, q.len()),
            None => {
                self.cursor = origin;
                self.anchor = None;
            }
        }
        if let Mode::Find { found, .. } = &mut self.mode {
            *found = hit.is_some() || q.is_empty();
        }
    }

    pub(crate) fn search_step(&mut self, forward: bool) {
        let q = self.find_query();
        let from = if forward { self.cursor } else { self.selection().map_or(self.cursor, |(a, _)| a) };
        let hit = self.find(&q, from, forward);
        if let Some(p) = hit {
            self.select_match(p, q.len());
        }
        if let Mode::Find { found, .. } = &mut self.mode {
            *found = hit.is_some() || q.is_empty();
        }
    }

    // ---------- Replace ----------

    pub(crate) fn start_replacing(&mut self, query: Vec<char>, with: String) {
        let start = self.buf.idx(self.cursor);
        self.buf.close_group();
        self.mode = Mode::Replacing { query, with, start, last: start, wrapped: false, count: 0 };
        self.replace_next();
    }

    fn finish_replacing(&mut self, count: usize) {
        self.mode = Mode::Normal;
        self.anchor = None;
        self.info(match count {
            0 => "No replacements".to_string(),
            1 => "Replaced 1 occurrence".to_string(),
            n => format!("Replaced {n} occurrences"),
        });
    }

    /// Selects the next match, or ends replacing once we are back where we started.
    fn replace_next(&mut self) {
        let Mode::Replacing { query, start, last, wrapped, count, .. } = &self.mode else { return };
        let (query, start, last, wrapped, count) = (query.clone(), *start, *last, *wrapped, *count);
        let Some(p) = self.find(&query, self.cursor, true) else {
            if count == 0 {
                self.mode = Mode::Normal;
                return self.error("No matches");
            }
            return self.finish_replacing(count);
        };
        let i = self.buf.idx(p);
        let wrapped = wrapped || i < last;
        if wrapped && i >= start {
            return self.finish_replacing(count);
        }
        self.select_match(p, query.len());
        if let Mode::Replacing { last: l, wrapped: w, .. } = &mut self.mode {
            *l = i;
            *w = wrapped;
        }
    }

    pub(crate) fn replacing_key(&mut self, k: crossterm::event::KeyEvent) {
        use crossterm::event::{KeyCode, KeyModifiers};
        let Mode::Replacing { query, with, count, .. } = &self.mode else { return };
        let (query, with, count) = (query.clone(), with.clone(), *count);
        match k.code {
            KeyCode::Char('y' | 'Y') | KeyCode::Enter => {
                if let Some((a, _)) = self.selection() {
                    let i = self.buf.idx(a);
                    self.buf.close_group();
                    self.insert(&with, None);
                    self.buf.close_group();
                    let delta = with.chars().count() as isize - query.len() as isize;
                    if let Mode::Replacing { start, last, count: c, .. } = &mut self.mode {
                        if i < *start {
                            *start = (*start as isize + delta).max(0) as usize;
                        }
                        *last = i;
                        *c += 1;
                    }
                }
                self.replace_next();
            }
            KeyCode::Char('n' | 'N') | KeyCode::Char(' ') => {
                self.anchor = None;
                self.replace_next();
            }
            KeyCode::Char('a' | 'A') => {
                let n = self.replace_all(&query, &with);
                self.finish_replacing(count + n);
            }
            KeyCode::Esc | KeyCode::Char('q') => self.finish_replacing(count),
            KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => self.finish_replacing(count),
            _ => {}
        }
    }

    /// Replaces every match in the document as one undo step.
    fn replace_all(&mut self, q: &[char], with: &str) -> usize {
        let ci = smart_case(q);
        let mut hits = Vec::new();
        let mut line = Vec::new();
        for l in 0..self.buf.lines() {
            self.buf.line_into(l, &mut line);
            let mut c = 0;
            while c + q.len() <= line.len() {
                if matches_at(&line, c, q, ci) {
                    hits.push(Pos::new(l, c));
                    c += q.len();
                } else {
                    c += 1;
                }
            }
        }
        if hits.is_empty() {
            return 0;
        }
        self.buf.close_group();
        self.buf.begin(self.cursor, None);
        for p in hits.iter().rev() {
            self.buf.delete(*p, Pos::new(p.line, p.col + q.len()));
            self.buf.insert(*p, with);
        }
        self.anchor = None;
        self.cursor = self.clamp(self.cursor);
        self.buf.end(self.cursor);
        hits.len()
    }

    // ---------- Go to line ----------

    pub(crate) fn goto_input(&mut self, s: &str) {
        match parse_goto(s) {
            Some((l, c)) => self.goto(l, c),
            None if s.trim().is_empty() => self.info("Cancelled"),
            None => self.error(format!("Not a line number: {s}")),
        }
    }

    // ---------- Line operations ----------

    pub(crate) fn duplicate(&mut self) {
        if !self.editable() {
            return;
        }
        let (first, last) = self.line_range();
        let block = self.buf.text(Pos::new(first, 0), Pos::new(last, self.buf.len(last)));
        let n = last - first + 1;
        self.buf.close_group();
        self.buf.begin(self.cursor, None);
        self.buf.insert(Pos::new(last, self.buf.len(last)), &format!("\n{block}"));
        self.cursor.line += n;
        if let Some(a) = &mut self.anchor {
            a.line += n;
        }
        self.buf.end(self.cursor);
    }

    pub(crate) fn move_lines(&mut self, down: bool) {
        if !self.editable() {
            return;
        }
        let (first, last) = self.line_range();
        if (!down && first == 0) || (down && last + 1 >= self.buf.lines()) {
            return;
        }
        let block = self.buf.text(Pos::new(first, 0), Pos::new(last, self.buf.len(last)));
        self.buf.close_group();
        self.buf.begin(self.cursor, None);
        if down {
            let next = self.buf.line_string(last + 1);
            let end = Pos::new(last + 1, self.buf.len(last + 1));
            self.buf.delete(Pos::new(first, 0), end);
            self.buf.insert(Pos::new(first, 0), &format!("{next}\n{block}"));
        } else {
            let prev = self.buf.line_string(first - 1);
            let end = Pos::new(last, self.buf.len(last));
            self.buf.delete(Pos::new(first - 1, 0), end);
            self.buf.insert(Pos::new(first - 1, 0), &format!("{block}\n{prev}"));
        }
        let shift = |p: Pos| Pos::new(if down { p.line + 1 } else { p.line - 1 }, p.col);
        self.cursor = shift(self.cursor);
        self.anchor = self.anchor.map(shift);
        self.buf.end(self.cursor);
    }

    pub(crate) fn toggle_comment(&mut self) {
        if !self.editable() {
            return;
        }
        let Some((open, close)) = self.lang.comment() else {
            return self.info(format!("No comment syntax for {}", self.lang.name()));
        };
        let op: Vec<char> = open.chars().collect();
        let cl: Vec<char> = close.chars().collect();
        let (first, last) = self.line_range();
        let lines: Vec<Vec<char>> = (first..=last).map(|l| self.buf.line(l)).collect();
        let indent_of = |l: &[char]| l.iter().take_while(|c| c.is_whitespace()).count();
        let trimmed_end = |l: &[char]| l.len() - l.iter().rev().take_while(|c| c.is_whitespace()).count();
        let mut targets: Vec<usize> =
            (0..lines.len()).filter(|&i| lines[i].iter().any(|c| !c.is_whitespace())).collect();
        let commented = |l: &[char]| {
            let (s, e) = (indent_of(l), trimmed_end(l));
            l[s..e].starts_with(&op) && (cl.is_empty() || (e - s >= op.len() + cl.len() && l[s..e].ends_with(&cl)))
        };
        let uncomment = !targets.is_empty() && targets.iter().all(|&i| commented(&lines[i]));
        let min_indent = targets.iter().map(|&i| indent_of(&lines[i])).min().unwrap_or(0);
        if targets.is_empty() {
            targets = (0..lines.len()).collect();
        }
        // (line, column, delta) – used to move the cursor and selection along
        let mut shifts: Vec<(usize, usize, isize)> = Vec::new();
        self.buf.close_group();
        self.buf.begin(self.cursor, None);
        for &i in &targets {
            let (l, line) = (first + i, &lines[i]);
            if uncomment {
                let s = indent_of(line);
                let e = trimmed_end(line);
                if !cl.is_empty() {
                    let mut a = e - cl.len();
                    if a > s + op.len() && line[a - 1] == ' ' {
                        a -= 1;
                    }
                    self.buf.delete(Pos::new(l, a), Pos::new(l, e));
                }
                let mut b = s + op.len();
                if line.get(b) == Some(&' ') {
                    b += 1;
                }
                self.buf.delete(Pos::new(l, s), Pos::new(l, b));
                shifts.push((l, s, -((b - s) as isize)));
            } else {
                if !cl.is_empty() {
                    self.buf.insert(Pos::new(l, line.len()), &format!(" {close}"));
                }
                self.buf.insert(Pos::new(l, min_indent), &format!("{open} "));
                shifts.push((l, min_indent, op.len() as isize + 1));
            }
        }
        let adjust = |p: Pos| match shifts.iter().find(|s| s.0 == p.line) {
            Some(&(_, c, d)) if p.col >= c => Pos::new(p.line, (p.col as isize + d).max(c as isize) as usize),
            _ => p,
        };
        self.cursor = self.clamp(adjust(self.cursor));
        self.anchor = self.anchor.map(|a| self.clamp(adjust(a)));
        self.buf.end(self.cursor);
    }

    // ---------- Brackets ----------

    /// The bracket at (or right before) the cursor and its partner.
    pub(crate) fn matching_bracket(&self) -> Option<(Pos, Pos)> {
        const PAIRS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];
        const LIMIT: usize = 200_000;
        let rope = self.buf.rope();
        let ci = self.buf.idx(self.cursor);
        for idx in [Some(ci), ci.checked_sub(1)].into_iter().flatten() {
            if idx >= rope.len_chars() {
                continue;
            }
            let ch = rope.char(idx);
            let Some(&(o, c)) = PAIRS.iter().find(|(o, c)| *o == ch || *c == ch) else { continue };
            let mut depth = 0usize;
            if ch == o {
                for (k, x) in rope.chars_at(idx).enumerate().take(LIMIT) {
                    if x == o {
                        depth += 1;
                    } else if x == c {
                        depth -= 1;
                        if depth == 0 {
                            return Some((self.buf.pos(idx), self.buf.pos(idx + k)));
                        }
                    }
                }
            } else {
                let mut it = rope.chars_at(idx + 1);
                let mut j = idx + 1;
                while let Some(x) = it.prev() {
                    j -= 1;
                    if x == c {
                        depth += 1;
                    } else if x == o {
                        depth -= 1;
                        if depth == 0 {
                            return Some((self.buf.pos(idx), self.buf.pos(j)));
                        }
                    }
                    if idx - j > LIMIT {
                        break;
                    }
                }
            }
            return None;
        }
        None
    }

    pub(crate) fn jump_bracket(&mut self) {
        match self.matching_bracket() {
            Some((_, m)) => {
                self.buf.close_group();
                self.set_cursor(m, false);
                self.want_x = None;
            }
            None => self.info("No matching bracket"),
        }
    }

    // ---------- JSON ----------

    /// Pretty-prints the selection (or the whole document); minifies if it is already pretty.
    pub(crate) fn format_json(&mut self) {
        if !self.editable() {
            return;
        }
        let sel = self.selection();
        let (a, b) = sel.unwrap_or((Pos::default(), self.buf.doc_end()));
        let src = self.buf.text(a, b);
        let result = match crate::json::pretty(&src, &self.indent.unit()) {
            Ok(p) if p == src => crate::json::minify(&src).map(|m| (m, "JSON minified")),
            Ok(p) => Ok((p, "JSON formatted")),
            Err(e) => Err(e),
        };
        match result {
            Ok((text, what)) => {
                let cursor = self.cursor;
                self.buf.close_group();
                self.buf.begin(cursor, None);
                self.buf.delete(a, b);
                let end = self.buf.insert(a, &text);
                if sel.is_some() {
                    self.anchor = Some(a);
                    self.cursor = end;
                } else {
                    self.anchor = None;
                    self.cursor = self.clamp(cursor);
                }
                self.buf.end(self.cursor);
                self.info(what);
            }
            Err(e) => self.error(format!("Invalid JSON: {e}")),
        }
    }

    // ---------- Shell commands, inserting files ----------

    /// Pipes the selection through `cmd` (replacing it), or inserts the command's output.
    pub(crate) fn run_command(&mut self, cmd: &str) {
        if cmd.trim().is_empty() {
            return self.info("Cancelled");
        }
        let input = self.selection().map(|(a, b)| self.buf.text(a, b)).unwrap_or_default();
        match fileio::pipe(cmd, &input) {
            Ok(mut out) => {
                if !input.ends_with('\n') && out.ends_with('\n') {
                    out.pop();
                }
                if out.is_empty() && input.is_empty() {
                    return self.info("Command produced no output");
                }
                self.buf.close_group();
                self.insert(&out, None);
                self.buf.close_group();
                self.info(format!("Ran: {cmd}"));
            }
            Err(e) => self.error(e),
        }
    }

    pub(crate) fn insert_file(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return self.info("Cancelled");
        }
        match fileio::load(&expand_tilde(name)) {
            Ok(Some(mut f)) => {
                let lines = f.text.lines().count();
                if f.final_newline {
                    f.text.push('\n');
                }
                self.buf.close_group();
                self.insert(&f.text, None);
                self.buf.close_group();
                self.info(format!("Inserted {lines} lines from {name}"));
            }
            Ok(None) => self.error(format!("File not found: {name}")),
            Err(e) => self.error(format!("{name}: {e}")),
        }
    }

    // ---------- Saving ----------

    pub(crate) fn save(&mut self) {
        if !self.editable() {
            return;
        }
        match self.path.clone() {
            Some(_) if self.lossy => {
                self.mode = Mode::Confirm {
                    question: "File was not valid UTF-8 – saving replaces invalid bytes with �. Save anyway? (y/n) "
                        .into(),
                    action: Action::SaveLossy,
                }
            }
            Some(p) if self.disk_changed() => {
                self.mode = Mode::Confirm {
                    question: "The file changed on disk since it was opened – overwrite? (y/n) ".into(),
                    action: Action::Write(p),
                }
            }
            Some(p) => self.write(p),
            None => self.mode = Mode::SaveAs(Prompt::default()),
        }
    }

    pub(crate) fn save_as(&mut self, input: String) {
        let s = input.trim();
        if s.is_empty() {
            return self.info("Cancelled");
        }
        let path = expand_tilde(s);
        if path.is_dir() {
            self.error(format!("\"{}\" is a directory", path.display()));
        } else if path.exists() {
            self.mode = Mode::Confirm {
                question: format!("\"{}\" already exists – overwrite? (y/n) ", path.display()),
                action: Action::Write(path),
            };
        } else {
            self.write(path);
        }
    }

    fn write(&mut self, path: PathBuf) {
        match fileio::save(&path, self.buf.rope()) {
            Ok(()) => self.saved(path),
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied && cfg!(unix) => {
                self.mode = Mode::Confirm {
                    question: "Permission denied – save with sudo? (y/n) ".into(),
                    action: Action::Sudo(path),
                }
            }
            Err(e) => self.error(format!("Error saving: {e}")),
        }
    }

    fn saved(&mut self, path: PathBuf) {
        self.buf.mark_saved();
        self.stamp = fileio::stamp(&path);
        self.disk_warned = false;
        self.lossy = false;
        self.info(format!("Saved {} ({} lines)", path.display(), self.buf.lines()));
        if self.path.as_ref() != Some(&path) {
            let first: String = self.buf.rope().line(0).chars().take(200).collect();
            self.lang = Lang::detect(Some(&path), &first);
            self.hl = vec![State::Normal];
            self.path = Some(path);
        }
    }

    pub(crate) fn run_action(&mut self, action: Action) {
        match action {
            Action::Write(p) => self.write(p),
            Action::SaveLossy => {
                self.lossy = false;
                self.save();
            }
            Action::Sudo(p) => {
                crate::term::leave();
                println!("nani: saving {} with sudo", p.display());
                let res = fileio::save_sudo(&p, self.buf.rope());
                let _ = crate::term::enter();
                match res {
                    Ok(()) => self.saved(p),
                    Err(e) => self.error(format!("sudo save failed: {e}")),
                }
            }
        }
    }

    // ---------- Watching the file ----------

    pub(crate) fn disk_changed(&self) -> bool {
        match (&self.path, self.stamp) {
            (Some(p), Some(s)) => fileio::stamp(p).is_some_and(|now| now != s),
            _ => false,
        }
    }

    /// Called about once a second while idle. Reloads the file if another program changed it
    /// and there are no unsaved changes; otherwise warns once. Returns true if a redraw is needed.
    pub fn tick(&mut self) -> bool {
        if !matches!(self.mode, Mode::Normal) || !self.disk_changed() {
            return false;
        }
        let Some(path) = self.path.clone() else { return false };
        if self.buf.dirty() {
            if self.disk_warned {
                return false;
            }
            self.disk_warned = true;
            self.error("The file changed on disk! Saving will ask before overwriting.");
            self.scroll_to_cursor(); // the message line takes a row
            return true;
        }
        match fileio::load(&path) {
            Ok(Some(f)) => {
                // as an undoable edit, so Ctrl+Z brings the old version back
                self.buf.close_group();
                self.buf.begin(self.cursor, None);
                self.buf.delete(Pos::default(), self.buf.doc_end());
                self.buf.insert(Pos::default(), &f.text);
                self.cursor = self.clamp(self.cursor);
                self.anchor = None;
                self.buf.end(self.cursor);
                self.buf.mark_saved();
                self.stamp = fileio::stamp(&path);
                self.lossy = f.lossy;
                self.info("Reloaded – the file changed on disk");
                self.scroll_to_cursor();
            }
            _ => self.stamp = None,
        }
        true
    }

    // ---------- View toggles, stats ----------

    pub(crate) fn toggle_wrap(&mut self) {
        self.wrap = !self.wrap;
        self.left = 0;
        self.top_seg = 0;
        self.info(if self.wrap { "Line wrap: on" } else { "Line wrap: off" });
        self.scroll_to_cursor();
    }

    pub(crate) fn toggle_numbers(&mut self) {
        self.numbers = !self.numbers;
        self.info(if self.numbers { "Line numbers: on" } else { "Line numbers: off" });
        self.scroll_to_cursor();
    }

    /// (words, characters) of the whole document.
    pub(crate) fn stats(&self) -> (usize, usize) {
        let rope = self.buf.rope();
        let mut words = 0;
        let mut in_word = false;
        for c in rope.chars() {
            let w = !c.is_whitespace();
            if w && !in_word {
                words += 1;
            }
            in_word = w;
        }
        (words, rope.len_chars())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ed(text: &str) -> Editor {
        Editor::new(text, None, None)
    }

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn find_wraps_both_ways() {
        let e = ed("Hallo Welt\nhallo du");
        assert_eq!(e.find(&chars("hallo"), Pos::new(0, 1), true), Some(Pos::new(1, 0)));
        assert_eq!(e.find(&chars("hallo"), Pos::new(1, 1), true), Some(Pos::new(0, 0)));
        assert_eq!(e.find(&chars("Hallo"), Pos::new(0, 1), true), Some(Pos::new(0, 0)));
        assert_eq!(e.find(&chars("hallo"), Pos::new(1, 0), false), Some(Pos::new(0, 0)));
        assert_eq!(e.find(&chars("hallo"), Pos::new(0, 0), false), Some(Pos::new(1, 0)));
        assert_eq!(e.find(&chars("xyz"), Pos::new(0, 0), true), None);
    }

    #[test]
    fn replace_all_is_one_undo_step() {
        let mut e = ed("a-a\na");
        assert_eq!(e.replace_all(&chars("a"), "bb"), 3);
        assert_eq!(e.buf.rope().to_string(), "bb-bb\nbb");
        e.buf.undo();
        assert_eq!(e.buf.rope().to_string(), "a-a\na");
    }

    #[test]
    fn comments_toggle() {
        let mut e = Editor::new("    let x;\n    let y;", Some(PathBuf::from("a.rs")), None);
        e.anchor = Some(Pos::new(0, 0));
        e.cursor = Pos::new(1, 3);
        e.toggle_comment();
        assert_eq!(e.buf.rope().to_string(), "    // let x;\n    // let y;");
        e.toggle_comment();
        assert_eq!(e.buf.rope().to_string(), "    let x;\n    let y;");
        let mut h = Editor::new("<p>x</p>", Some(PathBuf::from("a.html")), None);
        h.toggle_comment();
        assert_eq!(h.buf.rope().to_string(), "<!-- <p>x</p> -->");
        h.toggle_comment();
        assert_eq!(h.buf.rope().to_string(), "<p>x</p>");
    }

    #[test]
    fn line_moves_and_duplicates() {
        let mut e = ed("a\nb\nc");
        e.move_lines(true);
        assert_eq!(e.buf.rope().to_string(), "b\na\nc");
        assert_eq!(e.cursor.line, 1);
        e.duplicate();
        assert_eq!(e.buf.rope().to_string(), "b\na\na\nc");
        assert_eq!(e.cursor.line, 2);
        e.cursor = Pos::new(3, 0);
        e.move_lines(true);
        assert_eq!(e.buf.rope().to_string(), "b\na\na\nc");
    }

    #[test]
    fn brackets() {
        let mut e = ed("f(a[1], {b})");
        e.cursor = Pos::new(0, 1);
        assert_eq!(e.matching_bracket(), Some((Pos::new(0, 1), Pos::new(0, 11))));
        e.cursor = Pos::new(0, 12);
        assert_eq!(e.matching_bracket(), Some((Pos::new(0, 11), Pos::new(0, 1))));
    }

    #[test]
    fn lossy_file_asks_before_saving() {
        let mut e = Editor::new("gr\u{fffd}e", Some(PathBuf::from("/nonexistent-nani/l.txt")), None);
        e.lossy = true;
        e.save();
        assert!(matches!(e.mode, Mode::Confirm { action: Action::SaveLossy, .. }));
    }

    #[test]
    fn json_toggle() {
        let mut e = Editor::new(r#"{"a":[1,2]}"#, Some(PathBuf::from("x.json")), None);
        e.format_json();
        assert_eq!(e.buf.rope().to_string(), "{\n  \"a\": [\n    1,\n    2\n  ]\n}");
        e.format_json();
        assert_eq!(e.buf.rope().to_string(), r#"{"a":[1,2]}"#);
        // an empty selection (anchor == cursor) must not survive the shorter text
        e.format_json();
        e.cursor = e.buf.doc_end();
        e.anchor = Some(e.cursor);
        e.format_json();
        assert_eq!(e.anchor, None);
    }
}
