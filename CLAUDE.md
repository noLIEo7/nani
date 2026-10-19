# nani

A modern, fast, lightweight nano-style terminal text editor, written in Rust (crossterm, ropey, arboard).

## Commands
- Build: `cargo build` (release: `cargo build --release`)
- Test: `cargo test`
- Lint: `cargo clippy`
- Install: `./install.sh` (Linux/macOS), `install.ps1` (Windows)

## Conventions
- Talk to the user in German. Everything in the repo is English: code, comments, commit messages, UI strings, README and other docs. Existing German UI text or docs are legacy; translate them to English when touching that part (or when asked).
- Never commit secrets (API keys, tokens, `.env`).
- This repo is public — keep that in mind for anything committed.
