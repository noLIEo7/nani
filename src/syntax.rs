//! Lightweight, line-based syntax highlighting.
//!
//! Every line is tokenized on its own, with a small `State` carried over for constructs that
//! span lines (block comments, multi-line strings, code fences, tags). Colors are mapped to the
//! terminal's 16 standard colors when rendering, so nani follows the terminal theme.

use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Plain,
    Markdown,
    Json,
    Yaml,
    Toml,
    Ini,
    Shell,
    Python,
    Rust,
    JavaScript,
    Html,
    Css,
    C,
    Go,
    Lua,
    Sql,
    Dockerfile,
    Makefile,
    Diff,
    GitCommit,
    Csv,
    Log,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    Normal,
    Keyword,
    Type,
    Str,
    Number,
    Comment,
    Constant,
    Function,
    Key,
    Heading,
    Bold,
    Italic,
    Code,
    Link,
    Url,
    Tag,
    Attr,
    Added,
    Removed,
    Meta,
    Error,
    Warn,
    Info,
    Punct,
    Variable,
    Column(u8),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum State {
    #[default]
    Normal,
    Comment,
    Str(char),
    Triple(char),
    Fence,
    Tag,
    Css(u8),
    CssComment(u8),
}

impl Lang {
    pub fn name(self) -> &'static str {
        match self {
            Lang::Plain => "Plain text",
            Lang::Markdown => "Markdown",
            Lang::Json => "JSON",
            Lang::Yaml => "YAML",
            Lang::Toml => "TOML",
            Lang::Ini => "INI / config",
            Lang::Shell => "Shell",
            Lang::Python => "Python",
            Lang::Rust => "Rust",
            Lang::JavaScript => "JavaScript / TypeScript",
            Lang::Html => "HTML / XML",
            Lang::Css => "CSS",
            Lang::C => "C / C++",
            Lang::Go => "Go",
            Lang::Lua => "Lua",
            Lang::Sql => "SQL",
            Lang::Dockerfile => "Dockerfile",
            Lang::Makefile => "Makefile",
            Lang::Diff => "Diff",
            Lang::GitCommit => "Git commit message",
            Lang::Csv => "CSV",
            Lang::Log => "Log",
        }
    }

    /// Comment delimiters (open, close); close is empty for line comments.
    pub fn comment(self) -> Option<(&'static str, &'static str)> {
        match self {
            Lang::Rust | Lang::C | Lang::Go | Lang::JavaScript | Lang::Json => Some(("//", "")),
            Lang::Python
            | Lang::Shell
            | Lang::Yaml
            | Lang::Toml
            | Lang::Ini
            | Lang::Dockerfile
            | Lang::Makefile
            | Lang::GitCommit => Some(("#", "")),
            Lang::Lua | Lang::Sql => Some(("--", "")),
            Lang::Html | Lang::Markdown => Some(("<!--", "-->")),
            Lang::Css => Some(("/*", "*/")),
            Lang::Plain | Lang::Diff | Lang::Csv | Lang::Log => None,
        }
    }

    /// Indentation used when a file gives no hint: `None` = tabs, `Some(n)` = n spaces.
    pub fn default_indent(self) -> Option<usize> {
        match self {
            Lang::Makefile | Lang::Go => None,
            Lang::Yaml | Lang::Json => Some(2),
            _ => Some(4),
        }
    }

    pub fn detect(path: Option<&Path>, first_line: &str) -> Lang {
        if let Some(lang) = path.and_then(|p| p.file_name()).and_then(|n| n.to_str()).and_then(by_name) {
            return lang;
        }
        if let Some(rest) = first_line.strip_prefix("#!") {
            for (needle, lang) in [
                ("python", Lang::Python),
                ("node", Lang::JavaScript),
                ("deno", Lang::JavaScript),
                ("bun", Lang::JavaScript),
                ("lua", Lang::Lua),
                ("sh", Lang::Shell),
                ("fish", Lang::Shell),
            ] {
                if rest.contains(needle) {
                    return lang;
                }
            }
        }
        if first_line.starts_with("diff --git") {
            return Lang::Diff;
        }
        Lang::Plain
    }
}

fn by_name(name: &str) -> Option<Lang> {
    let lower = name.to_ascii_lowercase();
    let lang = match lower.as_str() {
        "commit_editmsg" | "merge_msg" | "tag_editmsg" | "squash_msg" => Lang::GitCommit,
        "makefile" | "gnumakefile" => Lang::Makefile,
        "dockerfile" | "containerfile" => Lang::Dockerfile,
        "cargo.lock" | "pipfile" | "poetry.lock" => Lang::Toml,
        ".bashrc" | ".bash_profile" | ".bash_aliases" | ".bash_logout" | ".profile" | ".zshrc" | ".zprofile"
        | ".zshenv" | ".zlogin" | ".zlogout" | "pkgbuild" | "apkbuild" => Lang::Shell,
        ".gitconfig" | ".gitmodules" | ".editorconfig" | ".npmrc" | ".pypirc" => Lang::Ini,
        _ if lower.starts_with("dockerfile.") || lower.ends_with(".dockerfile") => Lang::Dockerfile,
        _ if lower.starts_with(".env") => Lang::Ini,
        _ => {
            let ext = lower.rsplit_once('.')?.1;
            match ext {
                "md" | "markdown" | "mdown" | "mkd" => Lang::Markdown,
                "json" | "jsonc" | "json5" | "geojson" | "webmanifest" => Lang::Json,
                "yml" | "yaml" => Lang::Yaml,
                "toml" => Lang::Toml,
                "ini" | "conf" | "cfg" | "cnf" | "env" | "properties" | "service" | "socket" | "timer" | "mount"
                | "target" | "desktop" | "network" => Lang::Ini,
                "sh" | "bash" | "zsh" | "fish" | "ksh" | "ebuild" => Lang::Shell,
                "py" | "pyw" | "pyi" => Lang::Python,
                "rs" => Lang::Rust,
                "js" | "mjs" | "cjs" | "jsx" | "ts" | "tsx" | "mts" | "cts" => Lang::JavaScript,
                "html" | "htm" | "xhtml" | "xml" | "svg" | "plist" | "xsd" | "xsl" | "vue" | "svelte" => Lang::Html,
                "css" | "scss" | "less" => Lang::Css,
                "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" | "ino" => Lang::C,
                "go" => Lang::Go,
                "lua" => Lang::Lua,
                "sql" => Lang::Sql,
                "mk" | "mak" => Lang::Makefile,
                "diff" | "patch" => Lang::Diff,
                "csv" | "tsv" => Lang::Csv,
                "log" => Lang::Log,
                _ => return None,
            }
        }
    };
    Some(lang)
}

/// Highlights one line. `out` gets one style per char; returns the state for the next line.
pub fn highlight(lang: Lang, line: &[char], lnum: usize, state: State, out: &mut Vec<Style>) -> State {
    out.clear();
    out.resize(line.len(), Style::Normal);
    match lang {
        Lang::Plain => State::Normal,
        Lang::Markdown => markdown(line, state, out),
        Lang::Json => code(&JSON, line, state, out),
        Lang::Yaml => yaml(line, out),
        Lang::Toml => conf(false, line, state, out),
        Lang::Ini => conf(true, line, state, out),
        Lang::Shell => code(&SHELL, line, state, out),
        Lang::Python => code(&PYTHON, line, state, out),
        Lang::Rust => code(&RUST, line, state, out),
        Lang::JavaScript => code(&JS, line, state, out),
        Lang::Html => html(line, state, out),
        Lang::Css => css(line, state, out),
        Lang::C => code(&C, line, state, out),
        Lang::Go => code(&GO, line, state, out),
        Lang::Lua => code(&LUA, line, state, out),
        Lang::Sql => code(&SQL, line, state, out),
        Lang::Dockerfile => dockerfile(line, state, out),
        Lang::Makefile => makefile(line, out),
        Lang::Diff => diff(line, out),
        Lang::GitCommit => git_commit(line, lnum, out),
        Lang::Csv => csv(line, out),
        Lang::Log => log(line, out),
    }
}

// ---------- helpers ----------

fn at(line: &[char], i: usize, s: &str) -> bool {
    let mut j = i;
    for c in s.chars() {
        if line.get(j) != Some(&c) {
            return false;
        }
        j += 1;
    }
    true
}

fn fill(out: &mut [Style], a: usize, b: usize, s: Style) {
    let b = b.min(out.len());
    if a < b {
        out[a..b].fill(s);
    }
}

fn ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn word_end(line: &[char], mut i: usize) -> usize {
    while i < line.len() && ident(line[i]) {
        i += 1;
    }
    i
}

fn skip_ws(line: &[char], mut i: usize) -> usize {
    while i < line.len() && line[i].is_whitespace() {
        i += 1;
    }
    i
}

fn find(line: &[char], from: usize, c: char) -> Option<usize> {
    line.get(from..)?.iter().position(|&d| d == c).map(|p| from + p)
}

fn find_str(line: &[char], from: usize, s: &str) -> Option<usize> {
    (from..line.len()).find(|&j| at(line, j, s))
}

/// Index after the closing quote `q`, honoring backslash escapes.
fn string_end(line: &[char], mut i: usize, q: char) -> Option<usize> {
    while i < line.len() {
        if line[i] == '\\' {
            i += 2;
            continue;
        }
        if line[i] == q {
            return Some(i + 1);
        }
        i += 1;
    }
    None
}

fn triple_end(line: &[char], from: usize, q: char) -> Option<usize> {
    (from..line.len()).find(|&j| line[j] == q && line.get(j + 1) == Some(&q) && line.get(j + 2) == Some(&q)).map(|j| j + 3)
}

fn number_end(line: &[char], mut i: usize) -> usize {
    while i < line.len() && (ident(line[i]) || (line[i] == '.' && line.get(i + 1).is_some_and(|c| c.is_ascii_digit()))) {
        i += 1;
    }
    i
}

/// End of a `$VAR`, `${VAR}`, `$(cmd)` or `$1`-style variable starting at `i`.
fn var_end(line: &[char], i: usize) -> Option<usize> {
    let n = line.len();
    match line.get(i + 1)? {
        '{' => Some(find(line, i, '}').map_or(n, |p| p + 1)),
        '(' => Some(find(line, i, ')').map_or(n, |p| p + 1)),
        c if ident(*c) => Some(word_end(line, i + 1)),
        '?' | '#' | '@' | '*' | '!' | '$' | '-' => Some(i + 2),
        _ => None,
    }
}

/// Short ASCII word on the stack, mapped through `f` – avoids allocations while tokenizing.
struct SmallWord {
    buf: [u8; 24],
    len: usize,
}

impl SmallWord {
    fn new(w: &[char], f: fn(u8) -> u8) -> Option<Self> {
        if w.len() > 24 || !w.iter().all(char::is_ascii) {
            return None;
        }
        let mut buf = [0u8; 24];
        for (b, &c) in buf.iter_mut().zip(w) {
            *b = f(c as u8);
        }
        Some(SmallWord { buf, len: w.len() })
    }

    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

// ---------- C-like languages ----------

struct Spec {
    line_comment: &'static [&'static str],
    block_comment: Option<(&'static str, &'static str)>,
    quotes: &'static str,
    /// Quotes whose strings may continue on the next line.
    multiline: &'static str,
    triple: bool,
    keywords: &'static [&'static str],
    types: &'static [&'static str],
    constants: &'static [&'static str],
    ignore_case: bool,
    /// Shell rules: `#` only starts a comment at a word start, `$variables`.
    shell: bool,
    upper_types: bool,
    /// `"string":` is a key (JSON).
    keys: bool,
    /// Rust: char literals vs lifetimes, `macro!`, `#[attr]`.
    rust: bool,
    /// C preprocessor lines.
    preproc: bool,
}

const BASE: Spec = Spec {
    line_comment: &[],
    block_comment: None,
    quotes: "\"'",
    multiline: "",
    triple: false,
    keywords: &[],
    types: &[],
    constants: &[],
    ignore_case: false,
    shell: false,
    upper_types: false,
    keys: false,
    rust: false,
    preproc: false,
};

const RUST: Spec = Spec {
    line_comment: &["//"],
    block_comment: Some(("/*", "*/")),
    multiline: "\"",
    keywords: &[
        "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "fn", "for",
        "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "static", "struct",
        "super", "trait", "type", "union", "unsafe", "use", "where", "while", "yield", "self", "Self",
    ],
    types: &[
        "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize", "f32", "f64", "bool",
        "char", "str",
    ],
    constants: &["true", "false", "None", "Some", "Ok", "Err"],
    upper_types: true,
    rust: true,
    ..BASE
};

const C: Spec = Spec {
    line_comment: &["//"],
    block_comment: Some(("/*", "*/")),
    keywords: &[
        "auto", "break", "case", "const", "continue", "default", "do", "else", "enum", "extern", "for", "goto", "if",
        "inline", "register", "restrict", "return", "sizeof", "static", "struct", "switch", "typedef", "union",
        "volatile", "while", "class", "namespace", "template", "typename", "public", "private", "protected",
        "virtual", "override", "new", "delete", "this", "using", "try", "catch", "throw", "constexpr", "noexcept",
        "operator", "friend", "explicit", "mutable", "final",
    ],
    types: &[
        "void", "char", "short", "int", "long", "float", "double", "signed", "unsigned", "bool", "size_t", "ssize_t",
        "int8_t", "int16_t", "int32_t", "int64_t", "uint8_t", "uint16_t", "uint32_t", "uint64_t", "uintptr_t",
        "std", "string", "vector", "FILE",
    ],
    constants: &["true", "false", "NULL", "nullptr"],
    preproc: true,
    ..BASE
};

const GO: Spec = Spec {
    line_comment: &["//"],
    block_comment: Some(("/*", "*/")),
    quotes: "\"'`",
    multiline: "`",
    keywords: &[
        "break", "case", "chan", "const", "continue", "default", "defer", "else", "fallthrough", "for", "func", "go",
        "goto", "if", "import", "interface", "map", "package", "range", "return", "select", "struct", "switch",
        "type", "var",
    ],
    types: &[
        "bool", "byte", "complex64", "complex128", "error", "float32", "float64", "int", "int8", "int16", "int32",
        "int64", "rune", "string", "uint", "uint8", "uint16", "uint32", "uint64", "uintptr", "any",
    ],
    constants: &["true", "false", "nil", "iota"],
    ..BASE
};

const JS: Spec = Spec {
    line_comment: &["//"],
    block_comment: Some(("/*", "*/")),
    quotes: "\"'`",
    multiline: "`",
    keywords: &[
        "break", "case", "catch", "class", "const", "continue", "debugger", "default", "delete", "do", "else",
        "export", "extends", "finally", "for", "function", "if", "import", "in", "instanceof", "let", "new", "of",
        "return", "super", "switch", "this", "throw", "try", "typeof", "var", "void", "while", "with", "yield",
        "async", "await", "static", "get", "set", "from", "as", "type", "interface", "enum", "implements", "private",
        "public", "protected", "readonly", "declare", "namespace", "abstract", "keyof",
    ],
    types: &["string", "number", "boolean", "any", "unknown", "never", "object", "symbol", "bigint"],
    constants: &["true", "false", "null", "undefined", "NaN", "Infinity"],
    upper_types: true,
    ..BASE
};

const PYTHON: Spec = Spec {
    line_comment: &["#"],
    triple: true,
    keywords: &[
        "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif", "else", "except",
        "finally", "for", "from", "global", "if", "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass",
        "raise", "return", "try", "while", "with", "yield", "match", "case",
    ],
    types: &["int", "float", "str", "bool", "list", "dict", "set", "tuple", "bytes", "object", "type"],
    constants: &["True", "False", "None", "self", "cls"],
    upper_types: true,
    ..BASE
};

const LUA: Spec = Spec {
    line_comment: &["--"],
    block_comment: Some(("--[[", "]]")),
    keywords: &[
        "and", "break", "do", "else", "elseif", "end", "for", "function", "goto", "if", "in", "local", "not", "or",
        "repeat", "return", "then", "until", "while",
    ],
    constants: &["true", "false", "nil", "self"],
    ..BASE
};

const SQL: Spec = Spec {
    line_comment: &["--"],
    block_comment: Some(("/*", "*/")),
    keywords: &[
        "select", "from", "where", "insert", "into", "values", "update", "set", "delete", "create", "table", "drop",
        "alter", "add", "column", "index", "view", "join", "inner", "left", "right", "outer", "full", "cross", "on",
        "as", "and", "or", "not", "is", "in", "exists", "between", "like", "group", "by", "order", "having", "limit",
        "offset", "union", "all", "distinct", "case", "when", "then", "else", "end", "primary", "key", "foreign",
        "references", "default", "unique", "check", "constraint", "begin", "commit", "rollback", "transaction", "if",
        "returning", "with", "asc", "desc", "database", "schema", "grant", "revoke", "trigger", "procedure",
    ],
    types: &[
        "int", "integer", "bigint", "smallint", "text", "varchar", "char", "boolean", "bool", "date", "time",
        "timestamp", "timestamptz", "float", "real", "double", "numeric", "decimal", "serial", "blob", "json",
        "jsonb", "uuid",
    ],
    constants: &["true", "false", "null"],
    ignore_case: true,
    ..BASE
};

const SHELL: Spec = Spec {
    line_comment: &["#"],
    quotes: "\"'`",
    multiline: "\"'`",
    keywords: &[
        "if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case", "esac", "function", "in",
        "select", "return", "break", "continue", "local", "export", "readonly", "declare", "unset", "shift",
        "source", "alias", "set", "trap", "exit", "eval", "exec", "end", "not", "and", "or",
    ],
    constants: &["true", "false"],
    shell: true,
    ..BASE
};

const JSON: Spec = Spec {
    line_comment: &["//"],
    block_comment: Some(("/*", "*/")),
    quotes: "\"",
    constants: &["true", "false", "null"],
    keys: true,
    ..BASE
};

const DOCKER: &[&str] = &[
    "FROM", "RUN", "CMD", "LABEL", "EXPOSE", "ENV", "ADD", "COPY", "ENTRYPOINT", "VOLUME", "USER", "WORKDIR", "ARG",
    "ONBUILD", "STOPSIGNAL", "HEALTHCHECK", "SHELL", "MAINTAINER",
];

const MAKE: &[&str] = &[
    "include", "-include", "sinclude", "ifeq", "ifneq", "ifdef", "ifndef", "else", "endif", "define", "endef",
    "export", "unexport", "override", "vpath",
];

fn rust_char(line: &[char], i: usize) -> Option<usize> {
    match (line.get(i + 1), line.get(i + 2)) {
        (Some('\\'), _) => line.get(i + 2..)?.iter().take(10).position(|&c| c == '\'').map(|p| i + 2 + p + 1),
        (Some(_), Some('\'')) => Some(i + 3),
        _ => None,
    }
}

fn code(sp: &Spec, line: &[char], mut state: State, out: &mut [Style]) -> State {
    let n = line.len();
    let mut i = 0;
    let mut tok = 0; // start of the current string (for JSON keys)
    if sp.preproc && state == State::Normal {
        let s = skip_ws(line, 0);
        if line.get(s) == Some(&'#') {
            fill(out, s, n, Style::Meta);
            return State::Normal;
        }
    }
    while i < n {
        match state {
            State::Comment => {
                let close = sp.block_comment.map_or("*/", |b| b.1);
                let s = i;
                match find_str(line, i, close) {
                    Some(p) => {
                        i = p + close.chars().count();
                        state = State::Normal;
                    }
                    None => i = n,
                }
                fill(out, s, i, Style::Comment);
                continue;
            }
            State::Str(q) | State::Triple(q) => {
                let triple = matches!(state, State::Triple(_));
                let mut closed = false;
                while i < n {
                    let c = line[i];
                    if c == '\\' && !(sp.shell && q == '\'') {
                        fill(out, i, i + 2, Style::Str);
                        i += 2;
                        continue;
                    }
                    if sp.shell && q != '\'' && c == '$' {
                        if let Some(e) = var_end(line, i) {
                            fill(out, i, e, Style::Variable);
                            i = e;
                            continue;
                        }
                    }
                    if c == q && (!triple || (line.get(i + 1) == Some(&q) && line.get(i + 2) == Some(&q))) {
                        let e = i + if triple { 3 } else { 1 };
                        fill(out, i, e, Style::Str);
                        i = e;
                        closed = true;
                        break;
                    }
                    out[i] = Style::Str;
                    i += 1;
                }
                if closed {
                    state = State::Normal;
                    if sp.keys && line.get(skip_ws(line, i)) == Some(&':') {
                        fill(out, tok, i, Style::Key);
                    }
                } else if !triple && !sp.multiline.contains(q) {
                    state = State::Normal;
                }
                continue;
            }
            _ => {}
        }
        let c = line[i];
        if let Some((open, _)) = sp.block_comment {
            if at(line, i, open) {
                state = State::Comment;
                let e = i + open.chars().count();
                fill(out, i, e, Style::Comment);
                i = e;
                continue;
            }
        }
        if sp.line_comment.iter().any(|lc| at(line, i, lc)) && !(sp.shell && i > 0 && !line[i - 1].is_whitespace()) {
            fill(out, i, n, Style::Comment);
            return state;
        }
        if sp.quotes.contains(c) {
            if sp.rust && c == '\'' {
                match rust_char(line, i) {
                    Some(e) => {
                        fill(out, i, e, Style::Str);
                        i = e;
                    }
                    None => {
                        let e = word_end(line, i + 1);
                        fill(out, i, e, Style::Type);
                        i = e.max(i + 1);
                    }
                }
                continue;
            }
            tok = i;
            if sp.triple && line.get(i + 1) == Some(&c) && line.get(i + 2) == Some(&c) {
                fill(out, i, i + 3, Style::Str);
                state = State::Triple(c);
                i += 3;
            } else {
                out[i] = Style::Str;
                state = State::Str(c);
                i += 1;
            }
            continue;
        }
        if c.is_ascii_digit() && (i == 0 || !ident(line[i - 1])) {
            let e = number_end(line, i);
            fill(out, i, e, Style::Number);
            i = e;
            continue;
        }
        if sp.shell && c == '$' {
            if let Some(e) = var_end(line, i) {
                fill(out, i, e, Style::Variable);
                i = e;
                continue;
            }
        }
        if c == '@' && line.get(i + 1).is_some_and(|&d| ident_start(d)) {
            let e = word_end(line, i + 1);
            fill(out, i, e, Style::Meta);
            i = e;
            continue;
        }
        if sp.rust && c == '#' && matches!(line.get(i + 1), Some('[' | '!')) {
            let e = find(line, i, ']').map_or(n, |p| p + 1);
            fill(out, i, e, Style::Meta);
            i = e;
            continue;
        }
        if ident_start(c) {
            let e = word_end(line, i);
            let lower: fn(u8) -> u8 = if sp.ignore_case { |b| b.to_ascii_lowercase() } else { |b| b };
            let word = SmallWord::new(&line[i..e], lower);
            let w = word.as_ref().map_or("", |w| w.as_str());
            let style = if sp.keywords.contains(&w) {
                Style::Keyword
            } else if sp.types.contains(&w) {
                Style::Type
            } else if sp.constants.contains(&w) {
                Style::Constant
            } else if sp.rust && line.get(e) == Some(&'!') {
                fill(out, e, e + 1, Style::Function);
                Style::Function
            } else if sp.upper_types && c.is_uppercase() {
                Style::Type
            } else if line.get(skip_ws(line, e)) == Some(&'(') {
                Style::Function
            } else {
                Style::Normal
            };
            fill(out, i, e, style);
            i = e;
            continue;
        }
        i += 1;
    }
    state
}

// ---------- Markdown ----------

fn markdown(line: &[char], state: State, out: &mut [Style]) -> State {
    let n = line.len();
    let s = skip_ws(line, 0);
    let t = &line[s..];
    let fence = at(t, 0, "```") || at(t, 0, "~~~");
    if state == State::Fence {
        fill(out, 0, n, if fence { Style::Meta } else { Style::Code });
        return if fence { State::Normal } else { State::Fence };
    }
    if fence {
        fill(out, 0, n, Style::Meta);
        return State::Fence;
    }
    if t.first() == Some(&'#') {
        let h = t.iter().take_while(|&&c| c == '#').count();
        if h <= 6 && (h == t.len() || t[h] == ' ') {
            fill(out, 0, n, Style::Heading);
            return State::Normal;
        }
    }
    if t.len() >= 3
        && matches!(t[0], '-' | '*' | '_')
        && t.iter().all(|&c| c == t[0] || c == ' ')
        && t.iter().filter(|&&c| c == t[0]).count() >= 3
    {
        fill(out, 0, n, Style::Meta);
        return State::Normal;
    }
    if t.first() == Some(&'>') {
        fill(out, 0, n, Style::Comment);
        return State::Normal;
    }
    let mut i = s;
    if matches!(t.first(), Some('-' | '*' | '+')) && t.get(1) == Some(&' ') {
        out[s] = Style::Keyword;
        i = s + 2;
    } else {
        let d = t.iter().take_while(|c| c.is_ascii_digit()).count();
        if d > 0 && matches!(t.get(d), Some('.' | ')')) && t.get(d + 1) == Some(&' ') {
            fill(out, s, s + d + 1, Style::Keyword);
            i = s + d + 2;
        }
    }
    if at(line, i, "[ ]") || at(line, i, "[x]") || at(line, i, "[X]") {
        fill(out, i, i + 3, Style::Constant);
        i += 3;
    }
    md_inline(line, i, out);
    State::Normal
}

fn md_inline(line: &[char], mut i: usize, out: &mut [Style]) {
    let n = line.len();
    while i < n {
        let c = line[i];
        match c {
            '`' => {
                if let Some(p) = find(line, i + 1, '`') {
                    fill(out, i, p + 1, Style::Code);
                    i = p + 1;
                    continue;
                }
            }
            '*' | '_' => {
                let k = line[i..].iter().take_while(|&&d| d == c).take(3).count();
                let boundary = c == '*' || i == 0 || !line[i - 1].is_alphanumeric();
                if boundary && line.get(i + k).is_some_and(|d| !d.is_whitespace()) {
                    let mut j = i + k + 1;
                    while j + k <= n && !(line[j..j + k].iter().all(|&d| d == c) && !line[j - 1].is_whitespace()) {
                        j += 1;
                    }
                    if j + k <= n {
                        fill(out, i, j + k, if k >= 2 { Style::Bold } else { Style::Italic });
                        i = j + k;
                        continue;
                    }
                }
                i += k;
                continue;
            }
            '[' | '!' => {
                let b = if c == '!' { i + 1 } else { i };
                if line.get(b) == Some(&'[') {
                    if let Some(close) = find(line, b + 1, ']') {
                        if line.get(close + 1) == Some(&'(') {
                            if let Some(p) = find(line, close + 2, ')') {
                                fill(out, i, close + 1, Style::Link);
                                fill(out, close + 1, p + 1, Style::Url);
                                i = p + 1;
                                continue;
                            }
                        }
                    }
                }
            }
            'h' if at(line, i, "http://") || at(line, i, "https://") => {
                let e = (i..n).find(|&j| line[j].is_whitespace() || matches!(line[j], ')' | '>')).unwrap_or(n);
                fill(out, i, e, Style::Url);
                i = e;
                continue;
            }
            '<' if at(line, i + 1, "http") => {
                if let Some(p) = find(line, i, '>') {
                    fill(out, i, p + 1, Style::Url);
                    i = p + 1;
                    continue;
                }
            }
            '|' => out[i] = Style::Punct,
            _ => {}
        }
        i += 1;
    }
}

// ---------- YAML, TOML, INI ----------

const YAML_CONSTANTS: &[&str] =
    &["true", "false", "yes", "no", "on", "off", "null", "True", "False", "Yes", "No", "Null", "TRUE", "FALSE", "NULL"];
const CONF_CONSTANTS: &[&str] = &["true", "false", "yes", "no", "on", "off"];

/// Values: strings, numbers, constants, anchors/aliases, trailing comments.
fn scalars(line: &[char], mut i: usize, out: &mut [Style], constants: &[&str]) {
    let n = line.len();
    while i < n {
        let c = line[i];
        let word_start = i == 0 || line[i - 1].is_whitespace();
        if c == '#' && word_start {
            fill(out, i, n, Style::Comment);
            return;
        }
        if c == '"' || c == '\'' {
            let e = if c == '"' { string_end(line, i + 1, c) } else { find(line, i + 1, c).map(|p| p + 1) };
            let e = e.unwrap_or(n);
            fill(out, i, e, Style::Str);
            i = e;
            continue;
        }
        if (c == '&' || c == '*') && word_start && line.get(i + 1).is_some_and(|&d| ident(d)) {
            let e = word_end(line, i + 1);
            fill(out, i, e, Style::Type);
            i = e;
            continue;
        }
        if c == '!' && line.get(i + 1) == Some(&'!') {
            let e = word_end(line, i + 2);
            fill(out, i, e, Style::Meta);
            i = e;
            continue;
        }
        let num = c.is_ascii_digit() || (c == '-' && line.get(i + 1).is_some_and(|d| d.is_ascii_digit()));
        if num && (i == 0 || !ident(line[i - 1])) {
            let e = number_end(line, i + 1);
            fill(out, i, e, Style::Number);
            i = e;
            continue;
        }
        if ident_start(c) || c == '~' {
            let e = if c == '~' { i + 1 } else { word_end(line, i) };
            let w: String = line[i..e].iter().collect();
            if constants.contains(&w.as_str()) || (c == '~' && constants == YAML_CONSTANTS) {
                fill(out, i, e, Style::Constant);
            }
            i = e;
            continue;
        }
        i += 1;
    }
}

fn yaml_key(line: &[char], i: usize) -> Option<usize> {
    if matches!(line.get(i), Some('"' | '\'')) {
        let e = string_end(line, i + 1, line[i])?;
        return (line.get(e) == Some(&':')).then_some(e);
    }
    let mut j = i;
    while j < line.len() {
        match line[j] {
            ':' if matches!(line.get(j + 1), None | Some(' ')) => return (j > i).then_some(j),
            '#' if j > i && line[j - 1] == ' ' => return None,
            '"' | '\'' | '{' | '[' if j == i => return None,
            _ => {}
        }
        j += 1;
    }
    None
}

fn yaml(line: &[char], out: &mut [Style]) -> State {
    let n = line.len();
    if at(line, 0, "---") || at(line, 0, "...") {
        fill(out, 0, n, Style::Meta);
        return State::Normal;
    }
    let mut i = skip_ws(line, 0);
    while line.get(i) == Some(&'-') && matches!(line.get(i + 1), None | Some(' ')) {
        out[i] = Style::Keyword;
        i = skip_ws(line, i + 1);
    }
    if line.get(i) == Some(&'#') {
        fill(out, i, n, Style::Comment);
        return State::Normal;
    }
    if let Some(k) = yaml_key(line, i) {
        fill(out, i, k, Style::Key);
        i = k + 1;
    }
    scalars(line, i, out, YAML_CONSTANTS);
    State::Normal
}

fn conf(ini: bool, line: &[char], state: State, out: &mut [Style]) -> State {
    let n = line.len();
    let mut i;
    if let State::Triple(q) = state {
        match triple_end(line, 0, q) {
            Some(e) => {
                fill(out, 0, e, Style::Str);
                i = e;
            }
            None => {
                fill(out, 0, n, Style::Str);
                return state;
            }
        }
    } else {
        i = skip_ws(line, 0);
        match line.get(i) {
            Some('#') => {
                fill(out, i, n, Style::Comment);
                return State::Normal;
            }
            Some(';') if ini => {
                fill(out, i, n, Style::Comment);
                return State::Normal;
            }
            Some('[') => {
                let e = find(line, i, ']').map_or(n, |p| if line.get(p + 1) == Some(&']') { p + 2 } else { p + 1 });
                fill(out, i, e, Style::Type);
                i = e;
            }
            _ => {
                if at(line, i, "export ") {
                    fill(out, i, i + 6, Style::Keyword);
                    i = skip_ws(line, i + 7);
                }
                let sep = find(line, i, '=').or_else(|| {
                    if ini {
                        find(line, i, ':').filter(|&p| matches!(line.get(p + 1), None | Some(' ')))
                    } else {
                        None
                    }
                });
                if let Some(eq) = sep {
                    fill(out, i, eq, Style::Key);
                    i = eq + 1;
                }
            }
        }
        let v = skip_ws(line, i);
        if !ini && (at(line, v, "\"\"\"") || at(line, v, "'''")) {
            let q = line[v];
            match triple_end(line, v + 3, q) {
                Some(e) => {
                    fill(out, v, e, Style::Str);
                    i = e;
                }
                None => {
                    fill(out, v, n, Style::Str);
                    return State::Triple(q);
                }
            }
        }
    }
    scalars(line, i, out, CONF_CONSTANTS);
    State::Normal
}

// ---------- HTML / XML, CSS ----------

fn name_end(line: &[char], mut i: usize) -> usize {
    while i < line.len() && (ident(line[i]) || matches!(line[i], '-' | ':' | '.')) {
        i += 1;
    }
    i
}

fn html(line: &[char], mut state: State, out: &mut [Style]) -> State {
    let n = line.len();
    let mut i = 0;
    while i < n {
        match state {
            State::Comment => {
                let s = i;
                match find_str(line, i, "-->") {
                    Some(p) => {
                        i = p + 3;
                        state = State::Normal;
                    }
                    None => i = n,
                }
                fill(out, s, i, Style::Comment);
            }
            State::Tag => {
                let c = line[i];
                if c == '>' || at(line, i, "/>") || at(line, i, "?>") {
                    let e = if c == '>' { i + 1 } else { i + 2 };
                    fill(out, i, e, Style::Tag);
                    i = e;
                    state = State::Normal;
                } else if c == '"' || c == '\'' {
                    let e = find(line, i + 1, c).map_or(n, |p| p + 1);
                    fill(out, i, e, Style::Str);
                    i = e;
                } else if ident_start(c) {
                    let e = name_end(line, i);
                    fill(out, i, e, Style::Attr);
                    i = e;
                } else {
                    i += 1;
                }
            }
            _ => {
                if at(line, i, "<!--") {
                    state = State::Comment;
                    fill(out, i, i + 4, Style::Comment);
                    i += 4;
                } else if line[i] == '<'
                    && line.get(i + 1).is_some_and(|&d| matches!(d, '/' | '!' | '?') || ident_start(d))
                {
                    let mut e = i + 1;
                    if matches!(line[e], '/' | '!' | '?') {
                        e += 1;
                    }
                    let e = name_end(line, e);
                    fill(out, i, e, Style::Tag);
                    i = e;
                    state = State::Tag;
                } else if line[i] == '&' {
                    match find(line, i, ';').filter(|&p| {
                        p > i + 1 && p - i <= 10 && line[i + 1..p].iter().all(|c| c.is_alphanumeric() || *c == '#')
                    }) {
                        Some(p) => {
                            fill(out, i, p + 1, Style::Constant);
                            i = p + 1;
                        }
                        None => i += 1,
                    }
                } else {
                    i += 1;
                }
            }
        }
    }
    state
}

fn css_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '-' || c == '_'
}

fn css_word_end(line: &[char], mut i: usize) -> usize {
    while i < line.len() && css_ident(line[i]) {
        i += 1;
    }
    i
}

fn css(line: &[char], state: State, out: &mut [Style]) -> State {
    let (mut depth, mut comment) = match state {
        State::Css(d) => (d, false),
        State::CssComment(d) => (d, true),
        _ => (0, false),
    };
    let n = line.len();
    let selector_line = line.contains(&'{');
    let mut i = 0;
    while i < n {
        if comment {
            let s = i;
            match find_str(line, i, "*/") {
                Some(p) => {
                    i = p + 2;
                    comment = false;
                }
                None => i = n,
            }
            fill(out, s, i, Style::Comment);
            continue;
        }
        let c = line[i];
        let in_rules = depth > 0 && !selector_line;
        if at(line, i, "/*") {
            comment = true;
            fill(out, i, i + 2, Style::Comment);
            i += 2;
            continue;
        }
        match c {
            '{' => {
                depth = depth.saturating_add(1);
                i += 1;
            }
            '}' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            '"' | '\'' => {
                let e = find(line, i + 1, c).map_or(n, |p| p + 1);
                fill(out, i, e, Style::Str);
                i = e;
            }
            '@' => {
                let e = css_word_end(line, i + 1);
                fill(out, i, e, Style::Keyword);
                i = e;
            }
            '!' if at(line, i, "!important") => {
                fill(out, i, i + 10, Style::Keyword);
                i += 10;
            }
            '#' if in_rules => {
                let e = css_word_end(line, i + 1);
                fill(out, i, e, Style::Number);
                i = e;
            }
            ':' if in_rules => i += 1,
            c if (c.is_ascii_digit() || (c == '.' && line.get(i + 1).is_some_and(|d| d.is_ascii_digit())))
                && (i == 0 || !css_ident(line[i - 1])) =>
            {
                let mut e = i + 1;
                while e < n && (line[e].is_ascii_alphanumeric() || matches!(line[e], '.' | '%')) {
                    e += 1;
                }
                fill(out, i, e, Style::Number);
                i = e;
            }
            c if css_ident(c) || matches!(c, '.' | '#' | ':') => {
                let e = if css_ident(c) { css_word_end(line, i) } else { css_word_end(line, i + 1) };
                if !in_rules {
                    let style = match c {
                        '.' => Style::Type,
                        '#' => Style::Attr,
                        ':' => Style::Function,
                        _ => Style::Tag,
                    };
                    fill(out, i, e, style);
                } else if line.get(skip_ws(line, e)) == Some(&':') {
                    fill(out, i, e, Style::Key);
                } else if line.get(e) == Some(&'(') {
                    fill(out, i, e, Style::Function);
                }
                i = e.max(i + 1);
            }
            _ => i += 1,
        }
    }
    if comment {
        State::CssComment(depth)
    } else if depth > 0 {
        State::Css(depth)
    } else {
        State::Normal
    }
}

// ---------- Dockerfile, Makefile ----------

fn dockerfile(line: &[char], state: State, out: &mut [Style]) -> State {
    let s = skip_ws(line, 0);
    if state == State::Normal && line.get(s) == Some(&'#') {
        fill(out, s, line.len(), Style::Comment);
        return State::Normal;
    }
    let e = word_end(line, s);
    let mut start = 0;
    if state == State::Normal {
        if let Some(w) = SmallWord::new(&line[s..e], |b| b.to_ascii_uppercase()) {
            if DOCKER.contains(&w.as_str()) {
                fill(out, s, e, Style::Keyword);
                start = e;
            }
        }
    }
    code(&SHELL, &line[start..], state, &mut out[start..])
}

fn makefile(line: &[char], out: &mut [Style]) -> State {
    let n = line.len();
    if line.first() == Some(&'\t') {
        code(&SHELL, line, State::Normal, out);
        return State::Normal;
    }
    let s = skip_ws(line, 0);
    if line.get(s) == Some(&'#') {
        fill(out, s, n, Style::Comment);
        return State::Normal;
    }
    let mut e = s;
    while e < n && (ident(line[e]) || line[e] == '-') {
        e += 1;
    }
    let word: String = line[s..e].iter().collect();
    if MAKE.contains(&word.as_str()) {
        fill(out, s, e, Style::Keyword);
        code(&SHELL, &line[e..], State::Normal, &mut out[e..]);
        return State::Normal;
    }
    let colon = find(line, s, ':');
    let eq = find(line, s, '=');
    match (colon, eq) {
        (Some(c), eq) if eq.is_none_or(|q| c < q) && line.get(c + 1) != Some(&'=') => {
            fill(out, s, c, Style::Function);
            code(&SHELL, &line[c + 1..], State::Normal, &mut out[c + 1..]);
        }
        (_, Some(q)) => {
            let mut k = q;
            while k > s && matches!(line[k - 1], ':' | '?' | '+' | '!' | ' ') {
                k -= 1;
            }
            fill(out, s, k, Style::Key);
            code(&SHELL, &line[q + 1..], State::Normal, &mut out[q + 1..]);
        }
        _ => {
            code(&SHELL, line, State::Normal, out);
        }
    }
    State::Normal
}

// ---------- Diff, Git commit, CSV, Log ----------

fn diff(line: &[char], out: &mut [Style]) -> State {
    let style = if at(line, 0, "+++") || at(line, 0, "---") || at(line, 0, "diff ") || at(line, 0, "index ") {
        Style::Heading
    } else if at(line, 0, "@@") {
        Style::Meta
    } else if line.first() == Some(&'+') {
        Style::Added
    } else if line.first() == Some(&'-') {
        Style::Removed
    } else {
        Style::Normal
    };
    fill(out, 0, line.len(), style);
    State::Normal
}

fn git_commit(line: &[char], lnum: usize, out: &mut [Style]) -> State {
    let n = line.len();
    if line.first() == Some(&'#') {
        fill(out, 0, n, Style::Comment);
    } else if lnum == 0 {
        // summary line: keep it short, highlight what goes past 50 chars
        fill(out, 0, n.min(50), Style::Bold);
        fill(out, 50, n, Style::Warn);
    } else if lnum == 1 && n > 0 {
        // the second line should be empty
        fill(out, 0, n, Style::Error);
    }
    State::Normal
}

fn csv(line: &[char], out: &mut [Style]) -> State {
    let sep = if line.contains(&',') {
        ','
    } else if line.contains(&';') {
        ';'
    } else {
        '\t'
    };
    let mut col = 0u8;
    let mut quoted = false;
    for (i, &c) in line.iter().enumerate() {
        if c == '"' {
            quoted = !quoted;
        }
        if c == sep && !quoted {
            out[i] = Style::Punct;
            col = col.wrapping_add(1);
        } else {
            out[i] = Style::Column(col % 6);
        }
    }
    State::Normal
}

fn log(line: &[char], out: &mut [Style]) -> State {
    let n = line.len();
    let mut i = 0;
    while i < n {
        let c = line[i];
        if c.is_ascii_digit() && (i == 0 || !ident(line[i - 1])) {
            let e = (i..n)
                .find(|&j| !(line[j].is_ascii_digit() || matches!(line[j], ':' | '-' | '.' | ',' | '/' | 'T' | 'Z' | '+')))
                .unwrap_or(n);
            if e - i >= 5 && line[i..e].iter().any(|&d| matches!(d, ':' | '-' | '/')) {
                fill(out, i, e, Style::Number);
            }
            i = e;
            continue;
        }
        if c == '"' {
            let e = string_end(line, i + 1, '"').unwrap_or(n);
            fill(out, i, e, Style::Str);
            i = e;
            continue;
        }
        if ident_start(c) {
            let e = word_end(line, i);
            if let Some(w) = SmallWord::new(&line[i..e], |b| b.to_ascii_uppercase()) {
                let style = match w.as_str() {
                    "ERROR" | "ERR" | "FATAL" | "CRITICAL" | "CRIT" | "PANIC" | "EMERG" | "ALERT" | "FAIL"
                    | "FAILED" => Style::Error,
                    "WARN" | "WARNING" => Style::Warn,
                    "INFO" | "NOTICE" => Style::Info,
                    "DEBUG" | "TRACE" => Style::Comment,
                    _ => Style::Normal,
                };
                fill(out, i, e, style);
            }
            i = e;
            continue;
        }
        i += 1;
    }
    State::Normal
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hl(lang: Lang, s: &str, state: State) -> (Vec<Style>, State) {
        let chars: Vec<char> = s.chars().collect();
        let mut out = Vec::new();
        let st = highlight(lang, &chars, 0, state, &mut out);
        (out, st)
    }

    #[test]
    fn detection() {
        let d = |p: &str| Lang::detect(Some(Path::new(p)), "");
        assert_eq!(d("notes.md"), Lang::Markdown);
        assert_eq!(d("a/b/Cargo.toml"), Lang::Toml);
        assert_eq!(d(".zshrc"), Lang::Shell);
        assert_eq!(d("Makefile"), Lang::Makefile);
        assert_eq!(d(".git/COMMIT_EDITMSG"), Lang::GitCommit);
        assert_eq!(d(".env.local"), Lang::Ini);
        assert_eq!(d("x.txt"), Lang::Plain);
        assert_eq!(Lang::detect(Some(Path::new("script")), "#!/usr/bin/env python3"), Lang::Python);
    }

    #[test]
    fn rust_tokens() {
        let (s, st) = hl(Lang::Rust, "fn main() { let x = 42; // hi", State::Normal);
        assert_eq!(s[0], Style::Keyword);
        assert_eq!(s[3], Style::Function);
        assert_eq!(s[20], Style::Number);
        assert_eq!(s[24], Style::Comment);
        assert_eq!(st, State::Normal);
        let (_, st) = hl(Lang::Rust, "let s = /* open", State::Normal);
        assert_eq!(st, State::Comment);
        let (s, st) = hl(Lang::Rust, "still */ x", State::Comment);
        assert_eq!((s[0], s[9], st), (Style::Comment, Style::Normal, State::Normal));
        let (s, _) = hl(Lang::Rust, "'a' 'b", State::Normal);
        assert_eq!((s[1], s[5]), (Style::Str, Style::Type));
    }

    #[test]
    fn json_keys_and_values() {
        let (s, _) = hl(Lang::Json, r#"  "name": "nani", "n": 1, "ok": true"#, State::Normal);
        assert_eq!(s[3], Style::Key);
        assert_eq!(s[11], Style::Str);
        assert_eq!(s[23], Style::Number);
        assert_eq!(s[32], Style::Constant);
    }

    #[test]
    fn markdown_blocks() {
        assert_eq!(hl(Lang::Markdown, "# Title", State::Normal).0[2], Style::Heading);
        assert_eq!(hl(Lang::Markdown, "```rust", State::Normal).1, State::Fence);
        assert_eq!(hl(Lang::Markdown, "code", State::Fence).0[0], Style::Code);
        let (s, _) = hl(Lang::Markdown, "- a **b** `c` [d](e)", State::Normal);
        assert_eq!((s[0], s[5], s[11], s[15], s[18]), (Style::Keyword, Style::Bold, Style::Code, Style::Link, Style::Url));
        assert_eq!(hl(Lang::Markdown, "snake_case_word", State::Normal).0[6], Style::Normal);
    }

    #[test]
    fn config_formats() {
        let (s, _) = hl(Lang::Yaml, "  - name: \"x\" # c", State::Normal);
        assert_eq!((s[2], s[4], s[11], s[16]), (Style::Keyword, Style::Key, Style::Str, Style::Comment));
        let (s, _) = hl(Lang::Toml, "[package]", State::Normal);
        assert_eq!(s[1], Style::Type);
        let (s, _) = hl(Lang::Toml, "lto = true", State::Normal);
        assert_eq!((s[0], s[6]), (Style::Key, Style::Constant));
        assert_eq!(hl(Lang::Toml, "x = \"\"\"multi", State::Normal).1, State::Triple('"'));
        let (s, _) = hl(Lang::Shell, "echo \"$HOME\" # c", State::Normal);
        assert_eq!((s[6], s[13]), (Style::Variable, Style::Comment));
        assert_eq!(hl(Lang::Shell, "echo a#b", State::Normal).0[6], Style::Normal);
    }

    #[test]
    fn markup_and_logs() {
        let (s, st) = hl(Lang::Html, "<a href=\"x\">t</a> <!-- c", State::Normal);
        assert_eq!((s[1], s[3], s[8], s[12]), (Style::Tag, Style::Attr, Style::Str, Style::Normal));
        assert_eq!(st, State::Comment);
        let (s, st) = hl(Lang::Css, "a { color: #fff; }", State::Normal);
        assert_eq!((s[0], st), (Style::Tag, State::Normal));
        let (s, _) = hl(Lang::Css, "  margin: 10px;", State::Css(1));
        assert_eq!((s[2], s[10]), (Style::Key, Style::Number));
        let (s, _) = hl(Lang::Log, "2026-10-06 12:00:01 ERROR boom", State::Normal);
        assert_eq!((s[0], s[20]), (Style::Number, Style::Error));
        let (s, _) = hl(Lang::Diff, "+added", State::Normal);
        assert_eq!(s[0], Style::Added);
    }
}
