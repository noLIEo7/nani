# nani

A modern, fast and lightweight nano-style terminal text editor. Single binary, no configuration,
follows your terminal's colors (including transparency).

```sh
nani                 # empty document
nani notes.md        # open a file (created on first save)
nani +42 main.rs     # open at line 42 (also +42:7 for a column)
nani -v /etc/hosts   # read-only
git diff | nani -    # edit text from stdin
```

## Install

The repository is private, so every machine needs the GitHub CLI, logged in once:

| System | Get the GitHub CLI |
|---|---|
| CachyOS / Arch | `sudo pacman -S github-cli` |
| Fedora | `sudo dnf install gh` |
| Debian / Ubuntu | `sudo apt install gh` |
| macOS | `brew install gh` |
| Windows | `winget install GitHub.cli` |

```sh
gh auth login            # GitHub.com → HTTPS → login with a web browser
gh repo clone noLIEo7/nani
cd nani
```

Then:

- **Linux / macOS:** `sh install.sh` – builds nani if Rust is installed, otherwise downloads the
  latest release binary (force that with `sh install.sh --download`). Installs to `~/.local/bin`
  or `~/.cargo/bin` and optionally makes nani your default `$EDITOR` (bash, zsh, fish).
- **Windows (PowerShell):** `powershell -ExecutionPolicy Bypass -File install.ps1` – installs
  `nani.exe` to `%LOCALAPPDATA%\Programs\nani` and adds it to your PATH.

Check with `nani --version`. **Update:** `git pull` and run the installer again.
**Uninstall:** delete the `nani` binary.

Building from source needs Rust: `sudo pacman -S rust` (CachyOS), `sudo dnf install cargo` (Fedora),
https://rustup.rs (macOS, Windows).

### Releases

Pushing a version tag builds binaries for Linux (x86_64, static), macOS (Apple Silicon and Intel)
and Windows and publishes them as a GitHub release:

```sh
git tag v0.1.1 && git push origin v0.1.1
```

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
- Saves atomically, always as UTF-8 with LF line endings
- Offers to save with `sudo` when a file is not writable
- Notices when another program changes the file: reloads it if you have no unsaved changes,
  otherwise asks before overwriting
- System clipboard, bracketed paste, mouse selection and scrolling
- Large files stay fast (rope data structure; highlighting is skipped above 32 MB)
