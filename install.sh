#!/bin/sh
# Installs nani as the `nani` command: builds it with Rust if available, otherwise
# (or with --download) fetches the latest release binary with the GitHub CLI.
# Target: first of ~/.local/bin, ~/.cargo/bin that is on PATH (or $PREFIX).
set -e
cd "$(dirname "$0")"

if [ "$1" != "--download" ] && command -v cargo >/dev/null 2>&1; then
    cargo build --release
    bin=target/release/nani
elif command -v gh >/dev/null 2>&1; then
    case "$(uname -s)-$(uname -m)" in
        Linux-x86_64) asset=nani-linux-x86_64.tar.gz ;;
        Darwin-arm64) asset=nani-macos-arm64.tar.gz ;;
        Darwin-x86_64) asset=nani-macos-x86_64.tar.gz ;;
        *)
            echo "No prebuilt binary for $(uname -sm) – install Rust and run this script again."
            exit 1
            ;;
    esac
    tmp=$(mktemp -d)
    gh release download -p "$asset" -D "$tmp"
    tar -xzf "$tmp/$asset" -C "$tmp"
    bin="$tmp/nani"
else
    echo "Need either Rust (cargo) or the GitHub CLI (gh) – see README.md."
    exit 1
fi

if [ -n "$PREFIX" ]; then
    dest="$PREFIX"
else
    dest="$HOME/.local/bin"
    for d in "$HOME/.local/bin" "$HOME/.cargo/bin"; do
        case ":$PATH:" in *":$d:"*) dest="$d"; break ;; esac
    done
fi

mkdir -p "$dest"
install -m 755 "$bin" "$dest/nani"
echo "Installed: $dest/nani"

case ":$PATH:" in
    *":$dest:"*) ;;
    *)
        echo
        echo "Note: $dest is not on your PATH. Add it with:"
        echo "  bash: echo 'export PATH=\"$dest:\$PATH\"' >> ~/.bashrc"
        echo "  zsh:  echo 'export PATH=\"$dest:\$PATH\"' >> ~/.zshrc"
        echo "  fish: fish_add_path $dest"
        ;;
esac

# Optionally make nani the default editor (git commit, crontab -e, sudoedit, ...)
[ -t 0 ] || exit 0
[ "$EDITOR" = nani ] && exit 0
printf "\nUse nani as your default \$EDITOR? [y/N] "
read -r answer
case "$answer" in
    y | Y | yes) ;;
    *) exit 0 ;;
esac
case "$(basename "${SHELL:-sh}")" in
    fish)
        fish -c 'set -Ux EDITOR nani; set -Ux VISUAL nani'
        echo "Set EDITOR and VISUAL for fish."
        ;;
    *)
        case "$(basename "${SHELL:-sh}")" in
            zsh) rc="$HOME/.zshrc" ;;
            bash) rc="$HOME/.bashrc" ;;
            *) rc="$HOME/.profile" ;;
        esac
        if ! grep -qs 'EDITOR=nani' "$rc"; then
            printf '\nexport EDITOR=nani VISUAL=nani\n' >> "$rc"
        fi
        echo "Added 'export EDITOR=nani VISUAL=nani' to $rc – open a new terminal to use it."
        ;;
esac
