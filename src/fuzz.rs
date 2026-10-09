//! Randomized tests: drive the editor with random input and check that nothing panics
//! and the editor's invariants hold. Deterministic (fixed seeds), so failures reproduce.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use crate::buffer::Pos;
use crate::editor::Editor;
use crate::syntax::{highlight, Lang, State};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
        xs[self.below(xs.len())]
    }
}

const ALPHABET: &[char] = &[
    'a', 'b', 'x', 'y', 'n', 'q', 'Z', '0', '9', ' ', ' ', '\t', '\n', '\n', '{', '}', '(', ')', '[', ']', '"', '\'',
    '`', '#', '/', '*', '-', '+', '=', ':', ';', ',', '.', '<', '>', '!', '?', '\\', '$', '@', '%', '&', '|', '~', '_',
    'é', '日', 'ß', '\u{301}', '\u{1b}', '\r',
];

fn random_text(rng: &mut Rng, max: usize) -> String {
    let n = rng.below(max + 1);
    (0..n).map(|_| rng.pick(ALPHABET)).collect()
}

const LANGS: &[Lang] = &[
    Lang::Plain,
    Lang::Markdown,
    Lang::Json,
    Lang::Yaml,
    Lang::Toml,
    Lang::Ini,
    Lang::Shell,
    Lang::Python,
    Lang::Rust,
    Lang::JavaScript,
    Lang::Html,
    Lang::Css,
    Lang::C,
    Lang::Go,
    Lang::Lua,
    Lang::Sql,
    Lang::Dockerfile,
    Lang::Makefile,
    Lang::Diff,
    Lang::GitCommit,
    Lang::Csv,
    Lang::Log,
];

#[test]
fn highlighter_never_panics() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut out = Vec::new();
    for &lang in LANGS {
        let mut state = State::Normal;
        for lnum in 0..3000 {
            let line: Vec<char> = random_text(&mut rng, 60).chars().filter(|&c| c != '\n').collect();
            state = highlight(lang, &line, lnum, state, &mut out);
            assert_eq!(out.len(), line.len(), "{lang:?}: {:?}", line.iter().collect::<String>());
        }
    }
}

fn key(code: KeyCode, m: KeyModifiers) -> Event {
    Event::Key(KeyEvent::new(code, m))
}

fn random_event(rng: &mut Rng, ed: &Editor) -> Event {
    let none = KeyModifiers::NONE;
    let ctrl = KeyModifiers::CONTROL;
    let shift = KeyModifiers::SHIFT;
    let alt = KeyModifiers::ALT;
    match rng.below(20) {
        0..=5 => key(KeyCode::Char(rng.pick(ALPHABET)), none),
        6..=8 => {
            let code = rng.pick(&[
                KeyCode::Left,
                KeyCode::Right,
                KeyCode::Up,
                KeyCode::Down,
                KeyCode::Home,
                KeyCode::End,
                KeyCode::PageUp,
                KeyCode::PageDown,
            ]);
            key(code, rng.pick(&[none, ctrl, shift, ctrl | shift]))
        }
        9 => key(rng.pick(&[KeyCode::Enter, KeyCode::Tab, KeyCode::BackTab, KeyCode::Esc]), none),
        10 => key(rng.pick(&[KeyCode::Backspace, KeyCode::Delete]), rng.pick(&[none, ctrl, alt])),
        11 => key(rng.pick(&[KeyCode::Up, KeyCode::Down]), alt),
        // commands that neither touch files nor run programs
        12..=14 => key(
            KeyCode::Char(
                rng.pick(&['z', 'y', 'a', 'f', 'r', 'g', 'd', '/', 'b', 't', 'w', 'n', 'k', 'h', 'c', 'x', 'v']),
            ),
            ctrl,
        ),
        15 => key(KeyCode::Char('z'), ctrl | shift),
        16 => Event::Paste(random_text(rng, 30)),
        17 => Event::Resize(rng.below(120) as u16 + 1, rng.below(50) as u16 + 1),
        _ => {
            let kind = rng.pick(&[
                MouseEventKind::Down(MouseButton::Left),
                MouseEventKind::Drag(MouseButton::Left),
                MouseEventKind::Up(MouseButton::Left),
                MouseEventKind::ScrollDown,
                MouseEventKind::ScrollUp,
            ]);
            Event::Mouse(MouseEvent {
                kind,
                column: rng.below(ed.w as usize + 2) as u16,
                row: rng.below(ed.h as usize + 2) as u16,
                modifiers: rng.pick(&[none, shift]),
            })
        }
    }
}

fn valid(ed: &Editor, p: Pos) -> bool {
    p.line < ed.buf.lines() && p.col <= ed.buf.len(p.line)
}

#[test]
fn random_editing_keeps_invariants() {
    // NANI_FUZZ_SEEDS=5000 cargo test --release fuzz  – for a longer run
    let seeds = std::env::var("NANI_FUZZ_SEEDS").ok().and_then(|s| s.parse().ok()).unwrap_or(60u64);
    for seed in 1..=seeds {
        let mut rng = Rng(seed.wrapping_mul(0x2545_f491_4f6c_dd1d));
        let lang_file = rng.pick(&["a.txt", "a.rs", "a.py", "a.json", "a.html", "a.md", "a.css", "Makefile", "a.yaml"]);
        let original = random_text(&mut rng, 200);
        let mut ed = Editor::new(&original, Some(lang_file.into()), None);
        ed.resize(rng.below(100) as u16 + 1, rng.below(40) as u16 + 1);
        let mut sink = Vec::new();
        for step in 0..1500 {
            let ev = random_event(&mut rng, &ed);
            let desc = format!("seed {seed}, step {step}, event {ev:?}");
            // never leave the editor (Ctrl+Q is not generated, but a prompt could quit)
            ed.handle(ev);
            assert!(!ed.quit, "{desc}");
            assert!(valid(&ed, ed.cursor), "cursor {:?} out of range: {desc}", ed.cursor);
            if let Some(a) = ed.anchor {
                assert!(valid(&ed, a), "anchor {a:?} out of range: {desc}");
            }
            if step % 7 == 0 {
                sink.clear();
                ed.render(&mut sink).unwrap_or_else(|e| panic!("render failed: {e} ({desc})"));
            }
        }
        // undoing everything restores the original text, redoing everything the newest one
        while ed.buf.redo().is_some() {}
        let last = ed.buf.rope().to_string();
        while ed.buf.undo().is_some() {}
        assert_eq!(ed.buf.rope().to_string(), original, "seed {seed}: undo all");
        while ed.buf.redo().is_some() {}
        assert_eq!(ed.buf.rope().to_string(), last, "seed {seed}: redo all");
    }
}

/// Random edits, saves and changes made by "another program", with the file on disk.
#[test]
fn random_saving_and_reloading() {
    let dir = std::env::temp_dir().join(format!("nani-fuzz-files-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("f.txt");
    for seed in 1..=40u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        let disk = random_text(&mut rng, 100);
        std::fs::write(&path, &disk).unwrap();
        let f = crate::fileio::load(&path).unwrap().unwrap();
        let mut ed = Editor::new(&f.text, Some(path.clone()), None);
        ed.stamp = crate::fileio::stamp(&path);
        ed.resize(80, 24);
        for step in 0..300 {
            let desc = format!("seed {seed}, step {step}");
            match rng.below(10) {
                0 => {
                    while !matches!(ed.mode, crate::editor::Mode::Normal) {
                        ed.handle(key(KeyCode::Esc, KeyModifiers::NONE));
                    }
                    ed.handle(key(KeyCode::Char('s'), KeyModifiers::CONTROL));
                    if matches!(ed.mode, crate::editor::Mode::Confirm { .. }) {
                        ed.handle(key(KeyCode::Char('y'), KeyModifiers::NONE));
                    }
                    let on_disk = std::fs::read_to_string(&path).unwrap();
                    let text = ed.buf.rope().to_string();
                    let expected = if text.is_empty() { text } else { text + "\n" };
                    assert_eq!(on_disk, expected, "{desc}: saved text");
                    assert!(!ed.buf.dirty(), "{desc}: dirty after save");
                }
                1 => {
                    // another program rewrites the file (sleep so the modification time changes)
                    std::thread::sleep(std::time::Duration::from_millis(2));
                    let new = random_text(&mut rng, 100);
                    std::fs::write(&path, &new).unwrap();
                    while !matches!(ed.mode, crate::editor::Mode::Normal) {
                        ed.handle(key(KeyCode::Esc, KeyModifiers::NONE));
                    }
                    let dirty = ed.buf.dirty();
                    ed.tick();
                    if !dirty {
                        let expected = crate::fileio::decode(new.into_bytes()).text;
                        assert_eq!(ed.buf.rope().to_string(), expected, "{desc}: reload");
                        assert!(!ed.buf.dirty(), "{desc}: dirty after reload");
                    } else {
                        // unsaved changes are never thrown away
                        assert!(ed.buf.dirty(), "{desc}");
                    }
                }
                _ => {
                    let ev = random_event(&mut rng, &ed);
                    ed.handle(ev);
                    if !matches!(ed.mode, crate::editor::Mode::Normal) && rng.below(3) == 0 {
                        ed.handle(key(KeyCode::Esc, KeyModifiers::NONE));
                    }
                }
            }
            assert!(!ed.quit, "{desc}");
            assert!(valid(&ed, ed.cursor), "{desc}");
        }
    }
    std::fs::remove_dir_all(&dir).unwrap();
}
