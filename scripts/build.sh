#!/usr/bin/env bash
# Builds the Nexus addon DLL (cross-compiled from Linux with MinGW-w64).
#
#   scripts/build.sh                      build target/x86_64-pc-windows-gnu/release/pathing_editor.dll
#   scripts/build.sh --install <GW2 dir>  also copy it to <GW2 dir>/addons/
#
# Needs: rustup with the x86_64-pc-windows-gnu target, and mingw-w64-gcc.
set -euo pipefail
cd "$(dirname "$0")/.."

TARGET=x86_64-pc-windows-gnu
cargo build --release --target "$TARGET"
DLL="target/$TARGET/release/pathing_editor.dll"

# The DLL must only import system DLLs, never the MinGW runtime.
if x86_64-w64-mingw32-objdump -p "$DLL" | grep -iE 'DLL Name: (libstdc\+\+|libgcc|libwinpthread)'; then
    echo "error: $DLL depends on MinGW runtime DLLs (see above)" >&2
    exit 1
fi
echo "Built $DLL"

if [[ "${1:-}" == "--install" ]]; then
    addons="${2:?usage: scripts/build.sh --install <GW2 dir>}/addons"
    if [[ ! -d "$addons" ]]; then
        echo "error: $addons doesn't exist (is Nexus installed there?)" >&2
        exit 1
    fi
    cp "$DLL" "$addons/pathing_editor.dll"
    echo "Installed to $addons/pathing_editor.dll"
fi
