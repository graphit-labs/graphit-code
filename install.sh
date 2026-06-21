#!/usr/bin/env sh
# Graphit Code Installer
# Usage: curl -fsSL https://raw.githubusercontent.com/graphit-labs/graphit-code/main/install.sh | bash
set -e

REPO="graphit-labs/graphit-code"
BIN_NAME="graphit"
INSTALL_DIR="/usr/local/bin"

# ── Color helpers ────────────────────────────────────────────────────────────
if [ -t 1 ]; then
  BOLD='\033[1m'
  GREEN='\033[0;32m'
  CYAN='\033[0;36m'
  YELLOW='\033[0;33m'
  RED='\033[0;31m'
  RESET='\033[0m'
else
  BOLD=''; GREEN=''; CYAN=''; YELLOW=''; RED=''; RESET=''
fi

info()    { printf "${CYAN}  →${RESET} %s\n" "$*"; }
success() { printf "${GREEN}  ✓${RESET} %s\n" "$*"; }
warn()    { printf "${YELLOW}  ⚠${RESET} %s\n" "$*"; }
error()   { printf "${RED}  ✗${RESET} %s\n" "$*" >&2; exit 1; }

# ── Platform detection ───────────────────────────────────────────────────────
detect_platform() {
  OS="$(uname -s)"
  ARCH="$(uname -m)"

  case "$OS" in
    Linux)
      case "$ARCH" in
        x86_64)  PLATFORM="linux-amd64" ;;
        aarch64|arm64) PLATFORM="linux-arm64" ;;
        *) error "Unsupported architecture: $ARCH" ;;
      esac
      ;;
    Darwin)
      case "$ARCH" in
        arm64)   PLATFORM="darwin-arm64" ;;
        x86_64)  PLATFORM="darwin-amd64" ;;
        *) error "Unsupported architecture: $ARCH" ;;
      esac
      ;;
    *) error "Unsupported OS: $OS. Please install manually from https://github.com/$REPO/releases" ;;
  esac
}

# ── Dependency check ─────────────────────────────────────────────────────────
check_deps() {
  for cmd in curl tar; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
      error "Required tool not found: $cmd. Please install it and retry."
    fi
  done
}

# ── Fetch latest release tag ─────────────────────────────────────────────────
fetch_latest_version() {
  LATEST_URL="https://api.github.com/repos/${REPO}/releases/latest"
  VERSION="$(curl -fsSL "$LATEST_URL" | grep '"tag_name"' | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')"
  if [ -z "$VERSION" ]; then
    error "Could not determine latest version. Check your internet connection."
  fi
}

# ── Main ─────────────────────────────────────────────────────────────────────
printf "\n${BOLD}Graphit Code Installer${RESET}\n"
printf "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\n"

check_deps
detect_platform

info "Fetching latest version..."
fetch_latest_version
success "Latest version: $VERSION"

ARCHIVE_NAME="${BIN_NAME}-${PLATFORM}.tar.gz"
BASE_URL="https://github.com/${REPO}/releases/download/${VERSION}"
ARCHIVE_URL="${BASE_URL}/${ARCHIVE_NAME}"
CHECKSUM_URL="${BASE_URL}/checksums.sha256"

TMP_DIR="$(mktemp -d)"
TMP_ARCHIVE="${TMP_DIR}/${ARCHIVE_NAME}"
TMP_CHECKSUM="${TMP_DIR}/checksums.sha256"

# cleanup on exit
trap 'rm -rf "$TMP_DIR"' EXIT

info "Downloading ${ARCHIVE_NAME}..."
curl -fsSL --progress-bar "$ARCHIVE_URL" -o "$TMP_ARCHIVE" || \
  error "Failed to download archive from: $ARCHIVE_URL"
success "Downloaded archive"

info "Downloading checksums..."
curl -fsSL "$CHECKSUM_URL" -o "$TMP_CHECKSUM" || \
  error "Failed to download checksums from: $CHECKSUM_URL"

info "Verifying checksum..."
EXPECTED="$(grep "${ARCHIVE_NAME}" "$TMP_CHECKSUM" | awk '{print $1}')"
if [ -z "$EXPECTED" ]; then
  error "Checksum for ${ARCHIVE_NAME} not found in checksums.sha256"
fi

if command -v sha256sum >/dev/null 2>&1; then
  ACTUAL="$(sha256sum "$TMP_ARCHIVE" | awk '{print $1}')"
elif command -v shasum >/dev/null 2>&1; then
  ACTUAL="$(shasum -a 256 "$TMP_ARCHIVE" | awk '{print $1}')"
else
  warn "No sha256sum or shasum found — skipping checksum verification"
  ACTUAL="$EXPECTED"
fi

if [ "$ACTUAL" != "$EXPECTED" ]; then
  error "Checksum mismatch! Expected: $EXPECTED  Got: $ACTUAL"
fi
success "Checksum verified"

info "Extracting archive..."
tar -xzf "$TMP_ARCHIVE" -C "$TMP_DIR"

# Find the extracted binary (may be named graphit-linux-amd64 or graphit)
TMP_BIN=""
for candidate in "${TMP_DIR}/${BIN_NAME}-${PLATFORM}" "${TMP_DIR}/${BIN_NAME}"; do
  if [ -f "$candidate" ]; then
    TMP_BIN="$candidate"
    break
  fi
done
if [ -z "$TMP_BIN" ]; then
  error "Could not find binary in extracted archive"
fi
chmod +x "$TMP_BIN"
success "Archive extracted"

# ── Install ──────────────────────────────────────────────────────────────────
info "Installing to ${INSTALL_DIR}/${BIN_NAME}..."

if [ -w "$INSTALL_DIR" ]; then
  mv "$TMP_BIN" "${INSTALL_DIR}/${BIN_NAME}"
else
  sudo mv "$TMP_BIN" "${INSTALL_DIR}/${BIN_NAME}"
fi
success "Installed to ${INSTALL_DIR}/${BIN_NAME}"

# ── Verify ───────────────────────────────────────────────────────────────────
if command -v graphit >/dev/null 2>&1; then
  INSTALLED_VER="$(graphit version 2>/dev/null | head -1 || echo "$VERSION")"
  success "Verified: $INSTALLED_VER"
fi

# ── Next steps ───────────────────────────────────────────────────────────────
printf "\n${BOLD}${GREEN}Installation complete!${RESET}\n\n"
printf "  Next steps:\n\n"
printf "  ${CYAN}1.${RESET} Run initial setup:\n"
printf "     ${BOLD}graphit setup${RESET}\n\n"
printf "  ${CYAN}2.${RESET} Initialize your project:\n"
printf "     ${BOLD}graphit init --ide <antigravity|gemini|claude|cursor|kiro|codex|opencode>${RESET}\n\n"
printf "  ${CYAN}3.${RESET} Docs: ${BOLD}https://github.com/${REPO}/tree/main/docs${RESET}\n\n"
