#!/bin/sh
# Installs nani on Linux and macOS.
#
#   Without cloning:  curl -fsSL https://raw.githubusercontent.com/noLIEo7/nani/main/install.sh | sh
#   From a clone:     sh install.sh              (builds with Rust if installed, else downloads)
#
# Options: --download (never build), PREFIX=/some/dir (install location).
# Default location: the first of ~/.local/bin, ~/.cargo/bin that is on PATH (else ~/.local/bin).
set -e
repo=noLIEo7/nani

src=$(dirname "$0")
if [ "$1" != "--download" ] && grep -qs '^name = "nani"' "$src/Cargo.toml" && command -v cargo >/dev/null 2>&1; then
    (cd "$src" && cargo build --release --locked)
    bin="$src/target/release/nani"
else
    case "$(uname -s)-$(uname -m)" in
        Linux-x86_64 | Linux-amd64) asset=nani-linux-x86_64.tar.gz ;;
        Darwin-arm64) asset=nani-macos-arm64.tar.gz ;;
        Darwin-x86_64) asset=nani-macos-x86_64.tar.gz ;;
        *)
            echo "No prebuilt binary for $(uname -sm). Install Rust (https://rustup.rs), then run:"
            echo "  cargo install --git https://github.com/$repo"
            exit 1
            ;;
    esac
    url="https://github.com/$repo/releases/latest/download/$asset"
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    echo "Downloading $url"
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$url" -o "$tmp/$asset"
    elif command -v wget >/dev/null 2>&1; then
        wget -qO "$tmp/$asset" "$url"
    else
        echo "Need curl or wget to download nani."
        exit 1
    fi
    tar -xzf "$tmp/$asset" -C "$tmp"
    bin="$tmp/nani"
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
echo "Installed: $dest/nani ($("$dest/nani" --version))"

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

# Optionally make nani the default editor (git commit, crontab -e, sudoedit, ...).
# Reads the answer from the terminal, so this also works with `curl ... | sh`.
[ "$EDITOR" = nani ] && exit 0
(: </dev/tty) 2>/dev/null || exit 0
printf "\nUse nani as your default \$EDITOR? [y/N] "
read -r answer </dev/tty || answer=
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
