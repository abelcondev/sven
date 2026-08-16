#!/usr/bin/env bash
# install.sh — install the grok-sdd Spec-Driven Development layer for grok-build.
#
# Installs, without touching grok-build's Rust core:
#   1. the `grok-sdd` binary        → $BIN_DIR (default ~/.local/bin)
#   2. the 9 phase skills           → $GROK_HOME/skills/sdd-*
#   3. the Stop + PreToolUse hooks  → $GROK_HOME/hooks/sdd.json
#
# The binary is a self-contained Go program (no runtime deps). This script uses a
# prebuilt binary from ./dist if one matches your platform, otherwise builds from
# ./engine with `go` if it is installed.
#
# Env overrides:
#   GROK_HOME   grok config dir           (default: ~/.grok)
#   BIN_DIR     where to install grok-sdd  (default: ~/.local/bin)
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GROK_HOME="${GROK_HOME:-$HOME/.grok}"
BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
BIN="grok-sdd"

os="$(uname -s | tr '[:upper:]' '[:lower:]')"
arch="$(uname -m)"
case "$arch" in
  x86_64|amd64) arch="amd64" ;;
  arm64|aarch64) arch="arm64" ;;
esac

echo "grok-sdd installer"
echo "  GROK_HOME = $GROK_HOME"
echo "  BIN_DIR   = $BIN_DIR"
echo "  platform  = $os/$arch"
echo

# --- 1. resolve the binary ---------------------------------------------------
mkdir -p "$BIN_DIR"
prebuilt="$here/dist/${BIN}-${os}-${arch}"
if [[ -x "$prebuilt" ]]; then
  echo "→ using prebuilt binary $prebuilt"
  install -m 0755 "$prebuilt" "$BIN_DIR/$BIN"
elif command -v go >/dev/null 2>&1; then
  echo "→ building from source with $(go version | awk '{print $3}')"
  ( cd "$here/engine" && go build -o "$BIN_DIR/$BIN" . )
else
  echo "ERROR: no prebuilt binary for $os/$arch in ./dist and 'go' is not installed." >&2
  echo "       Install Go (https://go.dev/dl) and re-run, or run 'make crosscompile' on a machine that has Go and ship ./dist." >&2
  exit 1
fi
echo "  installed $BIN_DIR/$BIN"

# --- 2. skills ---------------------------------------------------------------
echo "→ installing phase skills to $GROK_HOME/skills"
mkdir -p "$GROK_HOME/skills"
for d in "$here"/grok/skills/sdd-*; do
  name="$(basename "$d")"
  rm -rf "$GROK_HOME/skills/$name"
  cp -R "$d" "$GROK_HOME/skills/$name"
  echo "  + skills/$name"
done

# --- 3. hooks ----------------------------------------------------------------
echo "→ installing hooks to $GROK_HOME/hooks/sdd.json"
mkdir -p "$GROK_HOME/hooks"
cp "$here/hooks/sdd.json" "$GROK_HOME/hooks/sdd.json"
echo "  + hooks/sdd.json (Stop: inject next step · PreToolUse: branch guard)"

# --- 4. example model config (never clobbers your real config.toml) ----------
echo "→ installing example model config to $GROK_HOME/config.example.toml"
cp "$here/grok/config.example.toml" "$GROK_HOME/config.example.toml"
echo "  + config.example.toml (menu of OpenAI-compatible / Anthropic / local providers)"

echo
echo "Done."
case ":$PATH:" in
  *":$BIN_DIR:"*) : ;;
  *) echo "NOTE: $BIN_DIR is not on your PATH. Add it, e.g.:"
     echo "      echo 'export PATH=\"$BIN_DIR:\$PATH\"' >> ~/.zshrc && source ~/.zshrc" ;;
esac
echo
echo "Next: in any project you want under Spec-Driven Development, run:"
echo "    grok-sdd init"
echo "Then talk to grok normally — it will follow the loop. 'grok-sdd next' shows the current step."
