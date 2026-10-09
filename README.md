# nani

A modern, fast and lightweight nano-style terminal text editor. Single binary, no configuration,
follows your terminal's colors (including transparency). Runs on Linux, macOS and Windows.

```sh
nani                 # empty document
nani notes.md        # open a file (created on first save)
nani +42 main.rs     # open at line 42 (also +42:7 for a column)
nani -v /etc/hosts   # read-only
git diff | nani -    # edit text from stdin
```

## Install

**Linux and macOS** – in a terminal:

```sh
curl -fsSL https://raw.githubusercontent.com/noLIEo7/nani/main/install.sh | sh
```

**Windows** – in PowerShell:

```powershell
irm https://raw.githubusercontent.com/noLIEo7/nani/main/install.ps1 | iex
```

That's it – check with `nani --version`. The installer downloads the latest release, puts `nani`
on your PATH and asks whether nani should become your default `$EDITOR` (used by `git commit`,
`crontab -e`, `sudoedit`, …).

### Other ways

- **From a clone** (builds from source if [Rust](https://rustup.rs) is installed, otherwise
  downloads the release):

  ```sh
  git clone https://github.com/noLIEo7/nani
  cd nani
  sh install.sh                                             # Linux, macOS
  powershell -ExecutionPolicy Bypass -File install.ps1      # Windows
  ```

- **With Cargo:** `cargo install --git https://github.com/noLIEo7/nani`
- **By hand:** download the archive for your system from
  [Releases](https://github.com/noLIEo7/nani/releases/latest), unpack it and put `nani`
  (`nani.exe`) in a folder on your PATH.

| Prebuilt binary | Runs on |
|---|---|
| `nani-linux-x86_64.tar.gz` | any 64-bit Linux distribution (statically linked) |
| `nani-macos-arm64.tar.gz` | Macs with Apple Silicon (M1 and newer) |
| `nani-macos-x86_64.tar.gz` | Intel Macs |
| `nani-windows-x86_64.zip` | Windows 10/11 (and Windows 11 on ARM) |

Anything else (Linux on ARM, BSD, …): install Rust and use `cargo install` as above.

### Where it goes, updating, uninstalling

| | Linux / macOS | Windows |
|---|---|---|
| Location | `~/.local/bin/nani` (or `~/.cargo/bin` if that is on your PATH; override with `PREFIX=/dir`) | `%LOCALAPPDATA%\Programs\nani\nani.exe` |
| Update | run the install command again | run the install command again |
| Uninstall | delete the file (and the `export EDITOR=nani` line in your shell rc, if you added it) | delete the folder |

## Keys

| Key | Action | Key | Action |
|---|---|---|---|
| Ctrl+S | Save | Ctrl+Q | Quit (twice discards changes) |
| Ctrl+C / X / V | Copy / cut / paste (no selection: whole line) | Ctrl+Z / Y | Undo / redo |
| Ctrl+F | Find | Ctrl+R | Replace (y / n / all) |
| Ctrl+G | Go to line[:column] | Ctrl+A | Select all |
| Ctrl+D | Duplicate line | Alt+↑ / ↓ | Move line |
| Ctrl+/ | Toggle comment | Ctrl+B | Jump to matching bracket |
| Ctrl+T | Format / minify JSON | Ctrl+E | Run command (pipes the selection through it) |
| Ctrl+P | Insert file | Ctrl+W | Toggle line wrap |
| Ctrl+N | Toggle line numbers | Ctrl+K / F1 | Help (language, indentation, word count) |
| Tab / Shift+Tab | Indent / unindent | Shift+Arrows, mouse | Select |

On macOS use **Ctrl**, not Cmd. Alt+↑/↓ needs "Use Option as Meta key" in Terminal.app.
Inside tmux, Ctrl+B is usually taken by tmux itself.

## Features

- Syntax highlighting for Markdown, JSON, YAML, TOML, INI/config/.env, Shell, Python, Rust,
  JavaScript/TypeScript, HTML/XML, CSS, C/C++, Go, Lua, SQL, Dockerfile, Makefile, Diff,
  Git commit messages, CSV and log files
- Detects the indentation style (tabs, 2 or 4 spaces) and auto-indents new lines
- Saves atomically, always as UTF-8 with LF line endings, and keeps the file's owner,
  group and permissions
- Offers to save with `sudo` when a file is not writable
- Asks before saving a file that was not valid UTF-8 (instead of silently replacing bytes)
- Notices when another program changes the file: reloads it if you have no unsaved changes,
  otherwise asks before overwriting
- System clipboard, bracketed paste, mouse selection and scrolling
- Large files stay fast (rope data structure; highlighting is skipped above 32 MB)

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release      # binary in target/release/nani
```

Every push and pull request runs the tests and clippy on Linux, macOS and Windows, plus a
longer randomized test run and ShellCheck for `install.sh`.

Pushing a version tag builds the binaries for all systems, publishes them as a GitHub release
and then checks the one-line installers on Linux, macOS and Windows:

```sh
git tag v0.1.2 && git push origin v0.1.2
```

## License

[MIT](LICENSE)
