#!/bin/bash
# SessionStart hook: prepare the repo so the Tauri backend compiles and tests run.
# Mirrors .github/workflows/test.yml. Runs only in Claude Code on the web.
set -euo pipefail

# Only run in the remote (web) environment; local machines manage their own setup.
if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

cd "${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel)}"

# 1. Initialize submodules. The backend depends on ../../glossia via a Cargo
#    path dependency, so it must be checked out before `cargo build`/`cargo test`.
git submodule update --init --recursive

# 2. Install the GTK/WebKit system libraries the Tauri backend links against.
#    Idempotent: apt-get install is a no-op when the packages are already present.
SUDO=""
if [ "$(id -u)" -ne 0 ]; then
  SUDO="sudo"
fi

export DEBIAN_FRONTEND=noninteractive
# Don't let an unrelated third-party PPA (e.g. deadsnakes) returning 403 abort
# the whole hook; the Ubuntu archive lists we need still refresh. The install
# below fails loudly if a package we actually require is unavailable.
$SUDO apt-get update || echo "session-start: warning: some apt sources failed to refresh; continuing."
$SUDO apt-get install -y --no-install-recommends \
  libgtk-3-dev \
  libsoup-3.0-dev \
  libwebkit2gtk-4.1-dev \
  libssl-dev \
  pkg-config

echo "session-start: submodules initialized and GTK/WebKit dependencies installed."
