mod buffer;
mod clipboard;
mod editor;
mod fileio;
mod json;
mod ops;
mod render;
mod syntax;
mod term;

use std::io::{self, IsTerminal, Read, Write};
use std::path::PathBuf;
use std::process::exit;
use std::time::Duration;

use crossterm::event;

use editor::{parse_goto, Editor};

const USAGE: &str = "Usage: nani [OPTIONS] [+LINE[:COL]] [--] [FILE]

  nani              open an empty document
  nani notes.md     open a file (or create it on first save)
  nani +42 file     open a file at line 42
  cmd | nani -      edit text from stdin
  nani -- -file     open a file whose name starts with -

Options:
  -v, --view        read-only mode
  -h, --help        show this help
  -V, --version     show the version

Press Ctrl+K (or F1) inside nani to see all key bindings.";

struct Args {
    file: Option<String>,
    readonly: bool,
    goto: Option<(usize, usize)>,
}

fn usage_error(msg: &str) -> ! {
    eprintln!("nani: {msg}\n\n{USAGE}");
    exit(2);
}

fn parse_args() -> Args {
    let mut args = Args { file: None, readonly: false, goto: None };
    let mut options = true;
    for arg in std::env::args().skip(1) {
        if !options {
            set_file(&mut args, arg);
            continue;
        }
        match arg.as_str() {
            // everything after `--` is a file name, even if it starts with `-` or `+`
            "--" => options = false,
            "-h" | "--help" => {
                println!("{USAGE}");
                exit(0);
            }
            "-V" | "--version" => {
                println!("nani {}", env!("CARGO_PKG_VERSION"));
                exit(0);
            }
            "-v" | "--view" => args.readonly = true,
            s if s.len() > 1 && s.starts_with('+') => match parse_goto(&s[1..]) {
                Some(g) => args.goto = Some(g),
                None => usage_error(&format!("invalid line number: {s}")),
            },
            s if s.len() > 1 && s.starts_with('-') => usage_error(&format!("unknown option: {s}")),
            s => set_file(&mut args, s.to_string()),
        }
    }
    args
}

fn set_file(args: &mut Args, name: String) {
    if args.file.is_some() {
        usage_error("only one file can be opened at a time");
    }
    args.file = Some(name);
}

fn open(file: Option<&str>) -> Editor {
    let Some(name) = file else {
        return Editor::new("", None, None);
    };
    if name == "-" {
        let mut bytes = Vec::new();
        if let Err(e) = io::stdin().read_to_end(&mut bytes) {
            eprintln!("nani: cannot read stdin: {e}");
            exit(1);
        }
        return Editor::new(&fileio::decode(bytes).text, None, Some("Read from stdin".into()));
    }
    let path = PathBuf::from(name);
    match fileio::load(&path) {
        Ok(Some(f)) => {
            let msg = if f.lossy {
                Some("Warning: not valid UTF-8 – invalid bytes were replaced".to_string())
            } else if f.crlf {
                Some("Windows line endings (CRLF) will be converted to LF on save".to_string())
            } else {
                None
            };
            let mut ed = Editor::new(&f.text, Some(path.clone()), msg);
            ed.stamp = fileio::stamp(&path);
            ed.lossy = f.lossy;
            ed
        }
        Ok(None) => Editor::new("", Some(path), Some(format!("New file: {name}"))),
        Err(e) => {
            eprintln!("nani: {name}: {e}");
            exit(1);
        }
    }
}

fn main() {
    let args = parse_args();
    if !io::stdout().is_terminal() {
        eprintln!("nani: must be run in a terminal");
        exit(1);
    }
    let mut ed = open(args.file.as_deref());
    ed.readonly = args.readonly;
    if let Err(e) = run(&mut ed, args.goto) {
        term::leave();
        eprintln!("nani: {e}");
        exit(1);
    }
}

fn run(ed: &mut Editor, goto: Option<(usize, usize)>) -> io::Result<()> {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        term::leave();
        hook(info);
    }));

    term::enter()?;
    let (w, h) = crossterm::terminal::size()?;
    ed.resize(w, h);
    if let Some((l, c)) = goto {
        ed.goto(l, c);
    }
    let mut out = io::BufWriter::with_capacity(1 << 16, io::stdout());
    let result = event_loop(ed, &mut out);
    let _ = out.flush();
    term::leave();
    result
}

fn event_loop(ed: &mut Editor, out: &mut impl Write) -> io::Result<()> {
    let mut redraw = true;
    loop {
        if redraw {
            ed.render(out)?;
        }
        if event::poll(Duration::from_secs(1))? {
            ed.handle(event::read()?);
            // Handle everything pending before redrawing (fast typing/pasting)
            while !ed.quit && event::poll(Duration::ZERO)? {
                ed.handle(event::read()?);
            }
            redraw = true;
        } else {
            // idle: check whether another program changed the file
            redraw = ed.tick();
        }
        if ed.quit {
            return Ok(());
        }
    }
}
