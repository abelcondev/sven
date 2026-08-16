#!/usr/bin/env bash
# grok-sdd one-line installer — Spec-Driven Development layer for grok-build.
#
#   curl -fsSL https://raw.githubusercontent.com/abelcondev/sven/main/sdd-ext/bootstrap.sh | bash
#
# Downloads a prebuilt binary + the skills/hooks bundle from the latest GitHub
# release. No Go, no git clone, no dependency on kez. Installs, touching no
# grok-build Rust core:
#   grok-sdd binary          → $BIN_DIR   (default ~/.local/bin)
#   phase skills             → $GROK_HOME/skills/sdd-*
#   Stop + PreToolUse hooks  → $GROK_HOME/hooks/sdd.json
#
# Env overrides: GROK_SDD_REPO, GROK_SDD_VERSION (tag, default latest),
#                GROK_HOME (~/.grok), BIN_DIR (~/.local/bin).
set -euo pipefail

REPO="${GROK_SDD_REPO:-abelcondev/sven}"
VERSION="${GROK_SDD_VERSION:-latest}"
GROK_HOME="${GROK_HOME:-$HOME/.grok}"
BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
BIN="grok-sdd"

os="$(uname -s | tr '[:upper:]' '[:lower:]')"
arch="$(uname -m)"
case "$arch" in
  x86_64|amd64) arch="amd64" ;;
  arm64|aarch64) arch="arm64" ;;
  *) echo "grok-sdd: unsupported arch '$arch'" >&2; exit 1 ;;
esac
case "$os" in
  darwin|linux) ;;
  *) echo "grok-sdd: unsupported OS '$os'" >&2; exit 1 ;;
esac

if [[ "$VERSION" == "latest" ]]; then
  base="https://github.com/$REPO/releases/latest/download"
else
  base="https://github.com/$REPO/releases/download/$VERSION"
fi

echo "grok-sdd installer"
echo "  repo     = $REPO ($VERSION)"
echo "  platform = $os/$arch"
echo "  BIN_DIR  = $BIN_DIR"
echo "  GROK_HOME= $GROK_HOME"
echo

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# --- binary ------------------------------------------------------------------
echo "→ downloading $BIN-$os-$arch"
if ! curl -fsSL "$base/$BIN-$os-$arch" -o "$tmp/$BIN"; then
  echo "ERROR: could not download the binary for $os/$arch from $base." >&2
  echo "       Check the release exists, or build from source: git clone https://github.com/$REPO && cd sven/sdd-ext && make build && ./install.sh" >&2
  exit 1
fi
chmod +x "$tmp/$BIN"
mkdir -p "$BIN_DIR"
install -m 0755 "$tmp/$BIN" "$BIN_DIR/$BIN"
echo "  installed $BIN_DIR/$BIN"

# --- skills + hooks bundle ---------------------------------------------------
echo "→ downloading skills + hooks bundle"
curl -fsSL "$base/$BIN-assets.tar.gz" -o "$tmp/assets.tar.gz"
tar xzf "$tmp/assets.tar.gz" -C "$tmp"
mkdir -p "$GROK_HOME/skills" "$GROK_HOME/hooks"
for d in "$tmp"/skills/sdd-*; do
  name="$(basename "$d")"
  rm -rf "$GROK_HOME/skills/$name"
  cp -R "$d" "$GROK_HOME/skills/$name"
done
cp "$tmp/hooks/sdd.json" "$GROK_HOME/hooks/sdd.json"
echo "  installed skills → $GROK_HOME/skills/sdd-*"
echo "  installed hooks  → $GROK_HOME/hooks/sdd.json"

echo
echo "Done."
case ":$PATH:" in
  *":$BIN_DIR:"*) : ;;
  *) echo "NOTE: $BIN_DIR is not on your PATH. Add it:"
     echo "      echo 'export PATH=\"$BIN_DIR:\$PATH\"' >> ~/.zshrc && source ~/.zshrc" ;;
esac
echo
echo "Next: in any project you want under Spec-Driven Development, run:"
echo "    grok-sdd init"
echo "Then talk to grok normally — it follows the loop. 'grok-sdd next' shows the current step."
