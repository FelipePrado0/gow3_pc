#!/usr/bin/env bash
# Downloads and extracts the latest prebuilt gow3-macos package for Apple Silicon or Intel Macs.
set -euo pipefail
cd -- "$(dirname -- "$0")/../.."

REPO="FelipePrado0/gow3_pc"
FALLBACK_REPO="Incognito-Coder/gow3_pc"
ZIP_FILE="gow3-macos.zip"

echo "============================================================"
echo " God of War III: Download Prebuilt macOS Package"
echo "============================================================"

# Try downloading via GitHub CLI if available
DOWNLOADED=0
if command -v gh >/dev/null 2>&1; then
    echo "==> Checking latest GitHub release via gh..."
    if gh release download --repo "$REPO" --pattern "$ZIP_FILE" -O "$ZIP_FILE" 2>/dev/null || \
       gh release download --repo "$FALLBACK_REPO" --pattern "$ZIP_FILE" -O "$ZIP_FILE" 2>/dev/null; then
        DOWNLOADED=1
    fi
fi

if [[ $DOWNLOADED -eq 0 ]]; then
    echo "==> Fetching latest release asset via curl..."
    RELEASE_URL="https://github.com/$REPO/releases/latest/download/$ZIP_FILE"
    if curl -fL --progress-bar "$RELEASE_URL" -o "$ZIP_FILE" 2>/dev/null || \
       curl -fL --progress-bar "https://github.com/$FALLBACK_REPO/releases/latest/download/$ZIP_FILE" -o "$ZIP_FILE" 2>/dev/null; then
        DOWNLOADED=1
    fi
fi

if [[ $DOWNLOADED -eq 0 ]]; then
    echo "==> Checking latest successful workflow artifact..."
    if command -v gh >/dev/null 2>&1; then
        RUN_ID=$(gh run list --workflow "macOS zip" --status success -L 1 --json databaseId -q '.[0].databaseId' 2>/dev/null || true)
        if [[ -n $RUN_ID ]]; then
            echo "==> Found successful workflow run $RUN_ID. Downloading artifact..."
            if gh run download "$RUN_ID" --name gow3-macos.zip --dir . 2>/dev/null; then
                DOWNLOADED=1
            fi
        fi
    fi
fi

if [[ $DOWNLOADED -eq 0 ]]; then
    echo "Error: Could not automatically download $ZIP_FILE." >&2
    echo "Please download 'gow3-macos.zip' manually from:" >&2
    echo "  https://github.com/$REPO/releases or https://github.com/$FALLBACK_REPO/actions" >&2
    echo "and extract it into this folder." >&2
    exit 1
fi

echo "==> Extracting $ZIP_FILE..."
unzip -q -o "$ZIP_FILE"
rm -f "$ZIP_FILE" "$ZIP_FILE.sha256" 2>/dev/null || true

if [[ -d gow3-macos ]]; then
    cp -R gow3-macos/* .
    rm -rf gow3-macos
fi

echo "==> Setting permissions and clearing quarantine..."
xattr -dr com.apple.quarantine . 2>/dev/null || true
if [[ -d bin ]]; then
    chmod -R u+rx bin 2>/dev/null || true
fi

echo ""
echo "============================================================"
echo " Download complete! Ready to run on macOS."
echo " Start the game with: bash run.sh"
echo "============================================================"
