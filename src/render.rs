//! Drawing the editor, the status bar and the help screen.
//!
//! Only foreground colors from the terminal's 16-color palette are used and the background is
//! never set, so nani follows the terminal theme, including transparency.

use std::io::{self, Write};
use std::rc::Rc;

use crossterm::style::{Attribute, Color, Print, SetAttribute, SetForegroundColor};
use crossterm::terminal::{Clear, ClearType};
use crossterm::{cursor, queue};

use crate::buffer::Pos;
use crate::editor::{cwidth, str_width, width, Editor, Mode, Prompt, RowInfo};
use crate::syntax::{highlight, Lang, State, Style};

const HINTS: &str = "^S Save  ^Q Quit  ^K Help ";
/// Above this size highlighting is switched off to keep big files snappy.
const HIGHLIGHT_LIMIT: usize = 32 << 20;
/// Lines longer than this (minified JSON, ...) are not highlighted.
const LONG_LINE: usize = 10_000;

/// Truncates/pads `s` to exactly `w` columns.
fn fit(s: &str, w: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let cw = cwidth(c);
        if used + cw > w {
            break;
        }
        out.push(c);
        used += cw;
    }
    out.extend(std::iter::repeat_n(' ', w - used));
    out
}

/// (color, bold, italic, underline)
fn look(s: Style) -> (Option<Color>, bool, bool, bool) {
    const COLUMNS: [Option<Color>; 6] = [
        None,
        Some(Color::DarkCyan),
        Some(Color::DarkYellow),
        Some(Color::DarkGreen),
        Some(Color::DarkMagenta),
        Some(Color::Blue),
    ];
    let c = |c| Some(c);
    match s {
        Style::Normal => (None, false, false, false),
        Style::Keyword => (c(Color::DarkMagenta), false, false, false),
        Style::Type | Style::Attr => (c(Color::DarkYellow), false, false, false),
        Style::Str | Style::Added | Style::Info => (c(Color::DarkGreen), false, false, false),
        Style::Number | Style::Constant | Style::Code | Style::Meta | Style::Variable => {
            (c(Color::DarkCyan), false, false, false)
        }
        Style::Comment | Style::Punct => (c(Color::DarkGrey), false, false, false),
        Style::Function | Style::Key => (c(Color::Blue), false, false, false),
        Style::Heading => (c(Color::Blue), true, false, false),
        Style::Bold => (None, true, false, false),
        Style::Italic => (None, false, true, false),
        Style::Link => (c(Color::Blue), false, false, true),
        Style::Url => (c(Color::DarkGrey), false, false, true),
        Style::Tag | Style::Removed => (c(Color::DarkRed), false, false, false),
        Style::Error => (c(Color::Red), true, false, false),
        Style::Warn => (c(Color::DarkYellow), true, false, false),
        Style::Column(i) => (COLUMNS[i as usize % 6], false, false, false),
    }
}

/// Look of one char: syntax style, selected, matching bracket.
type Look = (Style, bool, bool);

fn apply(out: &mut impl Write, (style, selected, bracket): Look) -> io::Result<()> {
    queue!(out, SetAttribute(Attribute::Reset))?;
    let (color, bold, italic, underline) = look(style);
    if let Some(c) = color {
        queue!(out, SetForegroundColor(c))?;
    }
    if bold || bracket {
        queue!(out, SetAttribute(Attribute::Bold))?;
    }
    if italic {
        queue!(out, SetAttribute(Attribute::Italic))?;
    }
    if underline || bracket {
        queue!(out, SetAttribute(Attribute::Underlined))?;
    }
    if selected {
        queue!(out, SetAttribute(Attribute::Reverse))?;
    }
    Ok(())
}

impl Editor {
    fn highlighting(&self) -> bool {
        self.lang != Lang::Plain && self.buf.rope().len_bytes() <= HIGHLIGHT_LIMIT
    }

    /// Highlighter state at the start of line `l`, computing missing states on the way.
    fn hl_state(&mut self, l: usize) -> State {
        let mut line = Vec::new();
        let mut styles = Vec::new();
        while self.hl.len() <= l {
            let i = self.hl.len() - 1;
            if self.buf.len(i) > LONG_LINE {
                self.hl.push(State::Normal);
                continue;
            }
            self.buf.line_into(i, &mut line);
            let next = highlight(self.lang, &line, i, self.hl[i], &mut styles);
            self.hl.push(next);
        }
        self.hl[l]
    }

    pub fn render(&mut self, out: &mut impl Write) -> io::Result<()> {
        if let Some(l) = self.buf.take_touched() {
            self.hl.truncate(l + 1);
        }
        if matches!(self.mode, Mode::Help) {
            return self.render_help(out);
        }
        let rows = self.text_rows();
        let g = self.gutter();
        let tw = self.text_width();
        let left = if self.wrap { 0 } else { self.left };
        let sel = self.selection();
        let n = self.buf.lines();
        let hl_on = self.highlighting();
        let brackets = if matches!(self.mode, Mode::Normal) { self.matching_bracket() } else { None };
        queue!(out, cursor::Hide)?;

        let mut row_map = vec![None; rows];
        let mut line: Rc<Vec<char>> = Rc::default();
        let mut styles = Vec::new();
        let mut segs: Rc<Vec<usize>> = Rc::default();
        let mut loaded = usize::MAX;
        let (mut l, mut s) = (self.top_line, self.top_seg);
        for (r, slot) in row_map.iter_mut().enumerate() {
            queue!(out, cursor::MoveTo(0, r as u16))?;
            if l < n {
                if loaded != l {
                    let (chars, rows) = self.cached(l, true);
                    line = chars;
                    segs = rows.unwrap_or_default();
                    if hl_on && line.len() <= LONG_LINE {
                        let st = self.hl_state(l);
                        highlight(self.lang, &line, l, st, &mut styles);
                    } else {
                        styles.clear();
                        styles.resize(line.len(), Style::Normal);
                    }
                    loaded = l;
                }
                let last = s + 1 == segs.len();
                let start = segs[s];
                let end = if last { line.len() } else { segs[s + 1] };
                if g > 0 {
                    if s == 0 {
                        let num = format!(" {:>w$} ", l + 1, w = g - 2);
                        if l == self.cursor.line {
                            queue!(out, Print(num))?;
                        } else {
                            queue!(out, SetForegroundColor(Color::DarkGrey), Print(num), SetAttribute(Attribute::Reset))?;
                        }
                    } else {
                        queue!(out, Print(" ".repeat(g)))?;
                    }
                }
                let row = RowInfo { line: l, start, end, last };
                self.draw_row(out, &line, &styles, row, sel, brackets, left, tw)?;
                *slot = Some(row);
                if last {
                    l += 1;
                    s = 0;
                } else {
                    s += 1;
                }
            }
            queue!(out, Clear(ClearType::UntilNewLine))?;
        }
        self.row_map = row_map;

        let w = self.w as usize;
        let status_y = self.h.saturating_sub(1);
        let mut prompt_x = None;
        if self.msg_visible() && status_y > 0 {
            let (text, cur_x, err) = self.message_line();
            prompt_x = cur_x;
            queue!(out, cursor::MoveTo(0, status_y - 1))?;
            if err {
                queue!(out, SetForegroundColor(Color::Red))?;
            }
            queue!(out, Print(fit(&text, w)), SetAttribute(Attribute::Reset))?;
        }
        self.render_status(out, status_y)?;

        let cur = match prompt_x {
            Some(x) => Some((x.min(w - 1) as u16, status_y - 1)),
            None => self.screen_cursor(g, left),
        };
        match cur {
            Some((x, y)) => queue!(out, cursor::MoveTo(x, y), cursor::Show)?,
            None => queue!(out, cursor::Hide)?,
        }
        out.flush()
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_row(
        &self,
        out: &mut impl Write,
        line: &[char],
        styles: &[Style],
        row: RowInfo,
        sel: Option<(Pos, Pos)>,
        brackets: Option<(Pos, Pos)>,
        left: usize,
        tw: usize,
    ) -> io::Result<()> {
        let l = row.line;
        let in_sel = |c: usize| sel.is_some_and(|(a, b)| (a..b).contains(&Pos::new(l, c)));
        let is_bracket = |c: usize| brackets.is_some_and(|(a, b)| a == Pos::new(l, c) || b == Pos::new(l, c));
        let normal: Look = (Style::Normal, false, false);
        let mut cur = normal;
        let mut s = String::new();
        let mut x = 0;
        for c in row.start..row.end {
            let ch = line[c];
            let cw = cwidth(ch);
            if x + cw <= left {
                x += cw;
                continue;
            }
            if x >= left + tw {
                break;
            }
            let look = (styles.get(c).copied().unwrap_or(Style::Normal), in_sel(c), is_bracket(c));
            if look != cur {
                queue!(out, Print(&s))?;
                s.clear();
                apply(out, look)?;
                cur = look;
            }
            if x < left || x + cw > left + tw {
                // wide character that is only partially visible
                let vis = (x + cw).min(left + tw) - x.max(left);
                s.extend(std::iter::repeat_n(' ', vis));
            } else {
                match ch {
                    '\t' => s.push_str("    "),
                    c if c.is_control() => s.push('?'),
                    c => s.push(c),
                }
            }
            x += cw;
        }
        // selected line break
        if row.last && in_sel(line.len()) && x >= left && x <= left + tw {
            let look = (Style::Normal, true, false);
            if look != cur {
                queue!(out, Print(&s))?;
                s.clear();
                apply(out, look)?;
                cur = look;
            }
            s.push(' ');
        }
        queue!(out, Print(&s))?;
        if cur != normal {
            queue!(out, SetAttribute(Attribute::Reset))?;
        }
        Ok(())
    }

    /// Text of the message line, the cursor column (for input) and whether it is an error.
    fn message_line(&self) -> (String, Option<usize>, bool) {
        let input = |label: &str, p: &Prompt, suffix: &str| {
            let text = format!("{label}{}{suffix}", p.string());
            (text, Some(str_width(label) + width(&p.text[..p.cur])), false)
        };
        match &self.mode {
            Mode::SaveAs(p) => input("Save as (Tab completes): ", p, ""),
            Mode::Find { p, found, .. } => input(
                "Find (↑↓ next, Enter done, Esc back): ",
                p,
                if *found { "" } else { "   [not found]" },
            ),
            Mode::ReplaceFind(p) => input("Replace – find: ", p, ""),
            Mode::ReplaceWith { p, .. } => input("Replace with: ", p, ""),
            Mode::Replacing { .. } => {
                let t = "Replace this match? y = yes, n = skip, a = all, Esc = stop ".to_string();
                let x = str_width(&t);
                (t, Some(x), false)
            }
            Mode::GoTo(p) => input("Go to line[:column]: ", p, ""),
            Mode::Command(p) => {
                let label = if self.selection().is_some() {
                    "Pipe selection through command: "
                } else {
                    "Insert output of command: "
                };
                input(label, p, "")
            }
            Mode::InsertFile(p) => input("Insert file (Tab completes): ", p, ""),
            Mode::Confirm { question, .. } => (question.clone(), Some(str_width(question)), true),
            Mode::Normal | Mode::Help => {
                let (m, err) = self.msg.clone().unwrap_or_default();
                (m, None, err)
            }
        }
    }

    fn render_status(&mut self, out: &mut impl Write, y: u16) -> io::Result<()> {
        let w = self.w as usize;
        let name = self.path.as_ref().map_or("[New]".to_string(), |p| p.display().to_string());
        let mut bar = format!(
            " {}{}{}   Ln {}, Col {}",
            name,
            if self.readonly { " [read-only]" } else { "" },
            if self.buf.dirty() { " •" } else { "" },
            self.cursor.line + 1,
            self.cursor.col + 1
        );
        if let Some((a, b)) = self.selection() {
            bar += &format!(" ({} selected)", self.buf.char_count(a, b));
        }
        bar += "   ";
        let btn_x = str_width(&bar);
        let btn = if self.wrap { "[Wrap: on]" } else { "[Wrap: off]" };
        bar += btn;
        let clamp = |v: usize| v.min(u16::MAX as usize) as u16;
        self.wrap_btn = (clamp(btn_x), clamp(btn_x + str_width(btn)));
        let used = str_width(&bar);
        if used + 2 + str_width(HINTS) <= w {
            bar += &" ".repeat(w - used - str_width(HINTS));
            bar += HINTS;
        }
        queue!(
            out,
            cursor::MoveTo(0, y),
            SetAttribute(Attribute::Reverse),
            Print(fit(&bar, w)),
            SetAttribute(Attribute::Reset)
        )
    }

    fn screen_cursor(&self, g: usize, left: usize) -> Option<(u16, u16)> {
        let c = self.cursor;
        let r = self.row_map.iter().position(|ri| {
            ri.is_some_and(|ri| ri.line == c.line && ri.start <= c.col && (c.col < ri.end || ri.last))
        })?;
        let ri = self.row_map[r]?;
        let line = self.line(c.line);
        let x = width(&line[ri.start..c.col]).checked_sub(left)?;
        Some(((g + x).min(self.w as usize - 1) as u16, r as u16))
    }

    fn help_lines(&self) -> Vec<String> {
        let name = self.path.as_ref().map_or("[New]".to_string(), |p| p.display().to_string());
        let (words, chars) = self.stats();
        let indent = format!(
            "{} ({})",
            self.indent.describe(),
            if self.indent_detected { "detected" } else { "default" }
        );
        let highlight = if self.lang != Lang::Plain && !self.highlighting() { " (too big to highlight)" } else { "" };
        [
            " nani – help".to_string(),
            String::new(),
            format!("  File       {name}{}", if self.readonly { " (read-only)" } else { "" }),
            format!("  Language   {}{highlight}", self.lang.name()),
            format!("  Indent     {indent}"),
            format!("  Size       {} lines · {words} words · {chars} characters", self.buf.lines()),
            "  Format     UTF-8 · LF".to_string(),
            String::new(),
            "  Ctrl+S  Save              Ctrl+Q  Quit (twice discards changes)".to_string(),
            "  Ctrl+C  Copy              Ctrl+X  Cut               Ctrl+V  Paste".to_string(),
            "  Ctrl+Z  Undo              Ctrl+Y  Redo              Ctrl+A  Select all".to_string(),
            "  Ctrl+F  Find              Ctrl+R  Replace           Ctrl+G  Go to line".to_string(),
            "  Ctrl+D  Duplicate line    Alt+↑↓  Move line         Ctrl+/  Toggle comment".to_string(),
            "  Ctrl+B  Matching bracket  Ctrl+T  Format JSON       Ctrl+E  Run command".to_string(),
            "  Ctrl+P  Insert file       Ctrl+W  Toggle wrap       Ctrl+N  Toggle line numbers".to_string(),
            "  Tab     Indent            Shift+Tab  Unindent       Ctrl+K / F1  This help".to_string(),
            String::new(),
            "  Copy and cut without a selection take the whole line.".to_string(),
            "  Shift+Arrows or the mouse select, Ctrl+Arrows move by word.".to_string(),
            "  Shift+drag uses the terminal's own selection.".to_string(),
            String::new(),
            "  Press any key to close.".to_string(),
        ]
        .into()
    }

    fn render_help(&mut self, out: &mut impl Write) -> io::Result<()> {
        let w = self.w as usize;
        let lines = self.help_lines();
        queue!(out, cursor::Hide)?;
        for r in 0..self.h {
            queue!(out, cursor::MoveTo(0, r))?;
            match lines.get(r as usize) {
                Some(l) if r == 0 => {
                    queue!(out, SetAttribute(Attribute::Reverse), Print(fit(l, w)), SetAttribute(Attribute::Reset))?
                }
                Some(l) => queue!(out, Print(fit(l, w)))?,
                None => {}
            }
            queue!(out, Clear(ClearType::UntilNewLine))?;
        }
        out.flush()
    }
}
