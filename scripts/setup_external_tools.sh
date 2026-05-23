#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TOOLS_DIR="$ROOT/external_tools"
TEFAS_REPO="$TOOLS_DIR/tefas-cli"
TEFAS_BIN="$TEFAS_REPO/target/release/cli"
# Optional: pin tefas-cli to a specific ref (tag/commit/branch). If set, the script
# will `git fetch` and `git checkout` that ref. Example: export TEFAS_REF=v1.2.3
TEFAS_REF=${TEFAS_REF:-}

echo "Setting up external tools in: $TOOLS_DIR"

mkdir -p "$TOOLS_DIR"

if [ ! -d "$TEFAS_REPO" ]; then
  echo "Cloning tefas-cli into $TEFAS_REPO..."
  git clone https://github.com/develooper1994/tefas-cli.git "$TEFAS_REPO"
else
  echo "tefas-cli already present; fetching latest changes..."
  (cd "$TEFAS_REPO" && git fetch --all) || true
fi

# If TEFAS_REF is provided, checkout that exact ref to make builds reproducible.
if [ -n "$TEFAS_REF" ]; then
  echo "Pinning tefas-cli to ref: $TEFAS_REF"
  (cd "$TEFAS_REPO" && git fetch --all --tags && git checkout --detach "$TEFAS_REF") || {
    echo "Failed to checkout TEFAS_REF=$TEFAS_REF" >&2
    exit 1
  }
else
  # If no pin requested, update to latest on the default branch.
  (cd "$TEFAS_REPO" && git pull --ff-only) || true
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "Error: 'cargo' not found in PATH. Install Rust toolchain to build tefas-cli." >&2
  exit 1
fi

pushd "$TEFAS_REPO" >/dev/null
echo "Building tefas-cli (release)..."
cargo build --release
popd >/dev/null

if [ -f "$TEFAS_BIN" ]; then
  echo "Built tefas-cli binary at: $TEFAS_BIN"
  echo
  echo "To use the CLI in smoke-tests, export the following environment variable:" 
  echo "  export TEFAS_CLI_CMD=$TEFAS_BIN"
else
  echo "Build completed but binary not found at: $TEFAS_BIN" >&2
  exit 1
fi

echo "Setup complete. You may now run smoke-tests as documented in docs/smoke_tests.md"
