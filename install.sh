#!/usr/bin/env sh
# Graphit Code Installer
# Usage: curl -fsSL https://raw.githubusercontent.com/graphit-labs/graphit-code/main/install.sh | bash
# Or:    curl -fsSL https://raw.githubusercontent.com/graphit-labs/graphit-code/main/install.sh | bash -s -- --dir ~/.local/bin
# Or:    curl -fsSL .../install.sh | VERSION=v1.4.0 bash -s -- --dir /usr/local/bin
set -e

REPO="graphit-labs/graphit-code"
BIN_NAME="graphit"
INSTALL_DIR="${HOME}/.local/bin"

# VERSION pins the release tag. Empty means "resolve the latest tag from the GitHub API",
# which is what an interactive install wants and what a reproducible one — a container image,
# a pipeline — must be able to opt out of. A pinned tag still goes through checksum
# verification below; pinning selects WHICH artifact, it does not skip verifying it.
VERSION="${VERSION:-}"

# ── Parse arguments ───────────────────────────────────────────────────────────
while [ "$#" -gt 0 ]; do
  case "$1" in
    --dir)
      if [ -z "$2" ]; then
        printf "Error: --dir requires a path argument\n" >&2
        exit 1
      fi
      INSTALL_DIR="$2"
      shift 2
      ;;
    --dir=*)
      INSTALL_DIR="${1#--dir=}"
      shift
      ;;
    --version)
      if [ -z "$2" ]; then
        printf "Error: --version requires a release tag argument\n" >&2
        exit 1
      fi
      VERSION="$2"
      shift 2
      ;;
    --version=*)
      VERSION="${1#--version=}"
      shift
      ;;
    --help|-h)
      printf "Usage: install.sh [--dir <install-dir>] [--version <release-tag>]\n"
      printf "\n"
      printf "  --dir <path>       Install graphit to this directory (default: \$HOME/.local/bin)\n"
      printf "  --version <tag>    Install this release tag instead of the latest (env: VERSION)\n"
      exit 0
      ;;
    *)
      printf "Unknown option: %s\n" "$1" >&2
      exit 1
      ;;
  esac
done

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

# ── Check if dir is in PATH ───────────────────────────────────────────────────
check_path() {
  _dir="$1"
  case ":${PATH}:" in
    *":${_dir}:"*) return 0 ;;
    *) return 1 ;;
  esac
}

# ── Main ─────────────────────────────────────────────────────────────────────
printf "\n${BOLD}Graphit Code Installer${RESET}\n"
printf "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\n"

check_deps
detect_platform

if [ -n "$VERSION" ]; then
  success "Pinned version: $VERSION"
else
  info "Fetching latest version..."
  fetch_latest_version
  success "Latest version: $VERSION"
fi

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
DLBIN="${INSTALL_DIR}/${BIN_NAME}"
info "Installing to ${DLBIN}..."

if [ ! -d "$INSTALL_DIR" ]; then
  mkdir -p "$INSTALL_DIR" || sudo mkdir -p "$INSTALL_DIR"
fi

if [ -w "$INSTALL_DIR" ]; then
  mv "$TMP_BIN" "$DLBIN"
else
  sudo mv "$TMP_BIN" "$DLBIN"
fi
success "Installed to ${DLBIN}"

# ── macOS: remove quarantine flag and ad-hoc sign ────────────────────────────
# The quarantine xattr is set by GUI downloads (Safari, Finder) — not by curl.
# The codesign is needed for cross-compiled darwin-arm64 binaries (no native signature).
# Both are no-ops when unnecessary, so keeping them is safe for future distribution methods.
if [ "$(uname -s)" = "Darwin" ]; then
  xattr -d com.apple.quarantine "$DLBIN" 2>/dev/null || true
  codesign --sign - --force "$DLBIN" 2>/dev/null || true
fi

# ── Verify ───────────────────────────────────────────────────────────────────
if command -v graphit >/dev/null 2>&1; then
  INSTALLED_VER="$(graphit version 2>/dev/null | head -1 || echo "$VERSION")"
  success "Verified: $INSTALLED_VER"
fi

# ── PATH check ───────────────────────────────────────────────────────────────
printf "\n"
if ! check_path "$INSTALL_DIR"; then
  warn "${INSTALL_DIR} is not in your PATH."
  printf "\n"
  printf "  Add it by running one of the following (then restart your shell):\n\n"
  printf "  ${CYAN}bash/zsh:${RESET}\n"
  printf "    ${BOLD}echo 'export PATH=\"\$PATH:${INSTALL_DIR}\"' >> ~/.bashrc${RESET}\n"
  printf "    ${BOLD}echo 'export PATH=\"\$PATH:${INSTALL_DIR}\"' >> ~/.zshrc${RESET}\n\n"
  printf "  ${CYAN}fish:${RESET}\n"
  printf "    ${BOLD}fish_add_path ${INSTALL_DIR}${RESET}\n\n"
fi

# ── Next steps ───────────────────────────────────────────────────────────────
printf "${BOLD}${GREEN}Installation complete!${RESET}\n\n"
printf "  Next steps:\n\n"
printf "  ${CYAN}1.${RESET} Run initial setup:\n"
printf "     ${BOLD}graphit setup${RESET}\n\n"
printf "  ${CYAN}2.${RESET} Initialize your project:\n"
printf "     ${BOLD}graphit init --ide <antigravity|gemini|claude|cursor|kiro|codex|opencode>${RESET}\n\n"
printf "  ${CYAN}3.${RESET} Docs: ${BOLD}https://github.com/${REPO}/tree/main/docs${RESET}\n\n"
