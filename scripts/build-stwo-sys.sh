#!/usr/bin/env bash
#
# Build stwo-sys (STWO FFI shim) as a cdylib and stage it for cipherocto
# deployment. Runs nightly-only cargo from the stwo-sys sub-project.
#
# Usage:
#   scripts/build-stwo-sys.sh                # release build, copy to dist/
#   scripts/build-stwo-sys.sh --debug        # debug build
#   scripts/build-stwo-sys.sh --skip-copy    # build only, do not stage
#
# Output:
#   target/release/libstwo_sys.so (Linux)
#   target/release/libstwo_sys.dylib (macOS)
#   target/release/libstwo_sys.dll (Windows)
#
# After build, copies artifact to dist/libstwo_sys.{so,dylib,dll} for
# inclusion in the cipherocto deployment tarball at /var/lib/cipherocto/.
#
# Requires:
#   - rustup with nightly-2025-06-23 toolchain installed
#     (`rustup toolchain install nightly-2025-06-23 --profile minimal`)
#   - On Linux: gcc + libc development headers (libc6-dev)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
STWO_SYS_DIR="$REPO_ROOT/crates/zk-vendor/stwo-sys"
DIST_DIR="$REPO_ROOT/dist"

PROFILE="release"
SKIP_COPY=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --debug)
            PROFILE="debug"
            shift
            ;;
        --skip-copy)
            SKIP_COPY=true
            shift
            ;;
        *)
            echo "Unknown arg: $1" >&2
            exit 1
            ;;
    esac
done

if [[ ! -d "$STWO_SYS_DIR" ]]; then
    echo "ERROR: stwo-sys dir not found: $STWO_SYS_DIR" >&2
    exit 1
fi

# Verify nightly toolchain available.
if ! rustup toolchain list 2>/dev/null | grep -q "nightly-2025-06-23"; then
    echo "Installing nightly-2025-06-23 (minimal profile)..." >&2
    rustup toolchain install nightly-2025-06-23 --profile minimal
fi

echo "Building stwo-sys ($PROFILE profile)..."
cd "$STWO_SYS_DIR"
cargo +nightly-2025-06-23 build "--$PROFILE"

if [[ "$SKIP_COPY" == "true" ]]; then
    echo "Build complete (skip-copy mode). Artifact at $STWO_SYS_DIR/target/$PROFILE/"
    exit 0
fi

# Stage artifact for deployment.
mkdir -p "$DIST_DIR"
ARTIFACT_DIR="$STWO_SYS_DIR/target/$PROFILE"
case "$(uname -s)" in
    Linux)
        ARTIFACT_NAME="libstwo_sys.so"
        ;;
    Darwin)
        ARTIFACT_NAME="libstwo_sys.dylib"
        ;;
    MINGW*|MSYS*|CYGWIN*)
        ARTIFACT_NAME="stwo_sys.dll"
        ;;
    *)
        echo "ERROR: unknown platform $(uname -s)" >&2
        exit 1
        ;;
esac

ARTIFACT_PATH="$ARTIFACT_DIR/$ARTIFACT_NAME"
if [[ ! -f "$ARTIFACT_PATH" ]]; then
    echo "ERROR: artifact not found: $ARTIFACT_PATH" >&2
    exit 1
fi

cp "$ARTIFACT_PATH" "$DIST_DIR/$ARTIFACT_NAME"
echo "Staged: $DIST_DIR/$ARTIFACT_NAME"
echo "Deploy to /var/lib/cipherocto/$ARTIFACT_NAME (or set CIPHEROCTO_STWO_LIB)."
