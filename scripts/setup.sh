#!/usr/bin/env bash
# One-shot environment setup for the NES clean-room pipeline (macOS / Linux).
# Idempotent: safe to re-run. Installs what is missing, builds nesrom and the
# sandbox image, and adjusts ~/.leviath/config.toml (with a backup).
#   scripts/setup.sh [--with-emulator] [--skip-docker]
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
WITH_EMU=0; SKIP_DOCKER=0
for a in "$@"; do case "$a" in --with-emulator) WITH_EMU=1;; --skip-docker) SKIP_DOCKER=1;; esac; done
ok()   { printf '  \033[1;32m✓\033[0m %s\n' "$*"; }
info() { printf '  \033[1;34m•\033[0m %s\n' "$*"; }
warn() { printf '  \033[1;33m!\033[0m %s\n' "$*"; }
have() { command -v "$1" >/dev/null 2>&1; }
OS="$(uname -s)"

echo "== package manager"
if [ "$OS" = "Darwin" ]; then
  have brew || { warn "Homebrew missing — installing"; /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"; }
  PKG="brew install"
else
  have apt-get && PKG="sudo apt-get install -y" || PKG=""
fi
ok "${PKG:-manual installs}"

echo "== Rust toolchain"
have cargo || { info "installing rustup"; curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y; . "$HOME/.cargo/env"; }
ok "cargo $(cargo --version | cut -d' ' -f2)"

echo "== cc65 (6502 C compiler)"
have cc65 || $PKG cc65
ok "cc65 $(cc65 --version 2>&1 | head -1)"

echo "== misc CLI (gh, jq, make, gcc, python3)"
have gh   || $PKG gh
have jq   || $PKG jq
have make || { [ "$OS" = "Darwin" ] && xcode-select --install || $PKG build-essential; }
have gcc  || { [ "$OS" = "Darwin" ] && xcode-select --install || $PKG build-essential; }
have python3 || $PKG python3
python3 -c 'import jsonschema' 2>/dev/null || python3 -m pip install --user --quiet jsonschema || pip3 install --user --quiet jsonschema
ok "gh $(gh --version | head -1 | cut -d' ' -f3), jq, make, gcc, python3+jsonschema"

echo "== Leviath"
if ! have lev; then info "installing Leviath"; curl -fsSL https://leviath.dev/install.sh | sh; fi
ok "$(lev --version 2>/dev/null | head -1)"
CFG="$HOME/.leviath/config.toml"
if [ ! -f "$CFG" ] || ! grep -q '^\[model_providers\|^default_provider' "$CFG"; then
  warn "no provider configured — running 'lev setup' (needs an Anthropic/OpenAI/Google/OpenRouter key)"; lev setup
fi
# Script tools (rom_* wrappers) must be able to shell out to nesrom even in a
# run whose model has shell = "deny". That is a machine-wide setting.
python3 - "$CFG" <<'PY'
import re, sys, shutil, time
p = sys.argv[1]; s = open(p).read(); orig = s
if "[tool_script_permissions]" not in s:
    s += '\n[tool_script_permissions]\nshell = "allow"\n'
else:
    head, _, rest = s.partition("[tool_script_permissions]")
    body, nxt, tail = rest.partition("\n[")
    if re.search(r'^\s*shell\s*=', body, re.M):
        body = re.sub(r'^(\s*shell\s*=\s*)"[^"]*"', r'\1"allow"', body, count=1, flags=re.M)
    else:
        body = body.rstrip("\n") + '\nshell = "allow"\n'
    s = head + "[tool_script_permissions]" + body + nxt + tail
if s != orig:
    shutil.copy(p, p + ".bak-" + time.strftime("%Y%m%d%H%M%S")); open(p, "w").write(s)
    print("  • config.toml: tool_script_permissions.shell = \"allow\" (backup written)")
else:
    print("  ✓ config.toml already allows script shell")
PY
lev doctor >/dev/null 2>&1 && ok "lev doctor passed" || warn "lev doctor reported problems — run 'lev doctor'"

echo "== nesrom (ROM analysis + asset extractor)"
( cd "$ROOT/tools/nesrom" && cargo install --path . --locked --quiet 2>/dev/null || cargo install --path . --quiet )
ok "nesrom $(nesrom --version 2>/dev/null || echo installed) at $(command -v nesrom)"
case ":$PATH:" in *":$HOME/.cargo/bin:"*) ;; *) warn "add \$HOME/.cargo/bin to your PATH (nesrom lives there)";; esac

echo "== sandbox image for Clean Room B (docker)"
if [ "$SKIP_DOCKER" = 1 ]; then warn "skipped (--skip-docker); writer/remedy/forge runs need nes-decomp-cc65:latest"
elif ! have docker; then warn "docker not installed — install Docker Desktop (or OrbStack) and re-run; writer runs need the sandbox image"
elif ! docker info >/dev/null 2>&1 && { [ "$OS" = "Darwin" ] && (open -a Docker 2>/dev/null || open -a OrbStack 2>/dev/null); for i in $(seq 1 30); do sleep 4; docker info >/dev/null 2>&1 && break; done; ! docker info >/dev/null 2>&1; }; then
  warn "docker daemon not running — start Docker Desktop / OrbStack and re-run setup"
else docker build -q -t nes-decomp-cc65:latest -f "$ROOT/docker/cc65.Dockerfile" "$ROOT/docker" >/dev/null && ok "nes-decomp-cc65:latest built"; fi

if [ "$WITH_EMU" = 1 ]; then
  echo "== emulator (optional, to eyeball built ROMs)"
  have fceux || $PKG fceux || warn "install FCEUX (brew install fceux / apt install fceux) for scripts/emu_check.sh"
  have fceux && ok "fceux $(fceux --version 2>/dev/null | head -1)"
fi

echo "== blueprints"
# fan-out workers are separate blueprints referenced by name (worker_agent); install them
for a in genesis-writer-worker genesis-reader-worker forge-worker; do lev add "$ROOT/agents/$a" >/dev/null 2>&1 && ok "$a installed (worker blueprints)" || warn "could not install $a"; done
for a in genesis-reader genesis-writer remedy forge; do
  lev validate "$ROOT/agents/$a" >/dev/null 2>&1 && ok "$a validates" || { warn "$a FAILS validation:"; lev validate "$ROOT/agents/$a" 2>&1 | tail -5; }
done
echo; echo "Setup complete. Try:  scripts/preflight.sh path/to/game.nes  then  scripts/pipeline.sh <name> path/to/game.nes"
