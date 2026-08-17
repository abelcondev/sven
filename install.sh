#!/usr/bin/env bash
#
# sven installer — builds the `grok` binary (with the native SDD loop compiled
# in) from source and installs it on your PATH.
#
#   curl -fsSL https://raw.githubusercontent.com/abelcondev/sven/main/install.sh | bash
#
# Overridable via environment:
#   SVEN_REPO_URL   git remote to clone            (default: this fork on GitHub)
#   SVEN_BRANCH     branch to build                (default: main)
#   SVEN_SRC        where the source is checked out (default: ~/.sven/src)
#   BIN_DIR         where the `grok` binary lands   (default: ~/.local/bin)
#
set -euo pipefail

REPO_URL="${SVEN_REPO_URL:-https://github.com/abelcondev/sven.git}"
BRANCH="${SVEN_BRANCH:-main}"
SRC_DIR="${SVEN_SRC:-$HOME/.sven/src}"
BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
BIN_NAME="grok"

# --- pretty output (plain when not a TTY) ------------------------------------
if [ -t 1 ]; then
  B=$'\033[1m'; G=$'\033[32m'; Y=$'\033[33m'; R=$'\033[31m'; DIM=$'\033[2m'; X=$'\033[0m'
else
  B=; G=; Y=; R=; DIM=; X=
fi
info() { printf '%s==>%s %s\n' "$G" "$X" "$*"; }
warn() { printf '%s!!%s  %s\n' "$Y" "$X" "$*"; }
die()  { printf '%serror:%s %s\n' "$R" "$X" "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

# --- 1. platform -------------------------------------------------------------
case "$(uname -s)" in
  Darwin | Linux) ;;
  *) die "the source installer supports macOS and Linux only (got $(uname -s)).
       On Windows, build manually — see the README's 'Building from source'." ;;
esac

# --- 2. prerequisites --------------------------------------------------------
have git || die "git is required. Install it and re-run."

# Put cargo's bin dir on PATH for this run so a fresh rustup/dotslash is visible.
case ":$PATH:" in *":$HOME/.cargo/bin:"*) ;; *) PATH="$HOME/.cargo/bin:$PATH" ;; esac

if ! have cargo; then
  printf '%serror:%s Rust (cargo) is required but was not found.\n' "$R" "$X" >&2
  printf '       Install the toolchain, then re-run this installer:\n' >&2
  printf '         %scurl --proto '"'"'=https'"'"' --tlsv1.2 -sSf https://sh.rustup.rs | sh%s\n' "$B" "$X" >&2
  exit 1
fi

# DotSlash resolves the hermetic bin/protoc used by proto codegen.
if ! have dotslash; then
  info "installing DotSlash (needed for hermetic protoc) via cargo…"
  cargo install dotslash
fi
have dotslash || die "dotslash is still not on PATH after install.
       Add ~/.cargo/bin to your PATH and re-run."

# --- 3. fetch / update the source -------------------------------------------
if [ -d "$SRC_DIR/.git" ]; then
  info "updating existing checkout at $SRC_DIR"
  git -C "$SRC_DIR" fetch --depth 1 origin "$BRANCH"
  git -C "$SRC_DIR" checkout -q "$BRANCH"
  git -C "$SRC_DIR" reset --hard -q "origin/$BRANCH"
else
  info "cloning $REPO_URL ($BRANCH) → $SRC_DIR"
  mkdir -p "$(dirname "$SRC_DIR")"
  git clone --depth 1 --branch "$BRANCH" "$REPO_URL" "$SRC_DIR"
fi

# --- 4. build ----------------------------------------------------------------
info "building the release binary — the first build takes a few minutes…"
( cd "$SRC_DIR" && cargo build -p xai-grok-pager-bin --release )

ARTIFACT="$SRC_DIR/target/release/xai-grok-pager"
[ -x "$ARTIFACT" ] || die "build finished but $ARTIFACT is missing."

# --- 5. install --------------------------------------------------------------
mkdir -p "$BIN_DIR"
install -m 0755 "$ARTIFACT" "$BIN_DIR/$BIN_NAME"
info "installed ${B}$BIN_DIR/$BIN_NAME${X}"

# --- 6. PATH hint + next steps ----------------------------------------------
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *)
    warn "$BIN_DIR is not on your PATH. Add it, e.g.:"
    printf '     echo '"'"'export PATH="%s:$PATH"'"'"' >> ~/.zshrc && exec zsh\n' "$BIN_DIR"
    ;;
esac

printf '\n%s✓ done.%s  run %sgrok --version%s to verify, or %sgrok%s to launch the TUI.\n' "$G" "$X" "$B" "$X" "$B" "$X"
printf '%sThe SDD loop ships inside this binary: launch grok, then drive the %ssdd%s%s tool (init · propose · next).%s\n' "$DIM" "$B$DIM" "$X" "$DIM" "$X"
