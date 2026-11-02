#!/usr/bin/env sh
set -eu

repo=graphit-labs/graphit-code
install_dir="${HOME}/.local/bin"
version=${VERSION:-}
archive=
fail() { printf 'Error: %s\n' "$*" >&2; exit 1; }
while [ "$#" -gt 0 ]; do
  case "$1" in
    --dir)
      if [ "$#" -lt 2 ] || [ -z "$2" ]; then fail "--dir requires a path"; fi
      install_dir=$2; shift 2 ;;
    --dir=*) install_dir=${1#--dir=}; shift ;;
    --version)
      if [ "$#" -lt 2 ] || [ -z "$2" ]; then fail "--version requires a release tag"; fi
      version=$2; shift 2 ;;
    --version=*) version=${1#--version=}; shift ;;
    --archive)
      if [ "$#" -lt 2 ] || [ -z "$2" ]; then fail "--archive requires a path"; fi
      archive=$2; shift 2 ;;
    --archive=*) archive=${1#--archive=}; [ -n "$archive" ] || fail "--archive requires a path"; shift ;;
    --help|-h)
      printf 'Usage: install.sh [--dir PATH] [--version TAG | --archive PATH]\n'
      printf 'Installs both Rust executables and their runtime assets. Default: ~/.local/bin\n'
      printf 'A local archive requires its adjacent .sha256 file and performs no download.\n'
      exit 0 ;;
    *) fail "Unknown option: $1" ;;
  esac
done
[ -n "$install_dir" ] || fail "Install directory must not be empty"
[ -z "$archive" ] || [ -z "$version" ] || fail "--archive and --version are mutually exclusive"
for tool in tar awk find mktemp; do
  command -v "$tool" >/dev/null 2>&1 || fail "Required tool not found: $tool"
done
if command -v sha256sum >/dev/null 2>&1; then
  hash_tool=sha256sum
elif command -v shasum >/dev/null 2>&1; then
  hash_tool=shasum
else
  fail "sha256sum or shasum is required to verify the release"
fi
case "$(uname -s):$(uname -m)" in
  Linux:x86_64) platform=linux-amd64 ;;
  Darwin:arm64) platform=darwin-arm64 ;;
  *) fail "No release target for this operating system and architecture" ;;
esac
archive_name="graphit-$platform.tar.gz"
if [ -n "$version" ]; then
  base_url="https://github.com/$repo/releases/download/$version"
else
  base_url="https://github.com/$repo/releases/latest/download"
fi
temp_dir=$(mktemp -d)
trap 'rm -rf "$temp_dir"' EXIT
if [ -n "$archive" ]; then
  if [ ! -s "$archive" ] || [ ! -s "$archive.sha256" ]; then
    fail "Local archive and adjacent .sha256 are required"
  fi
  cp "$archive" "$temp_dir/$archive_name"
  cp "$archive.sha256" "$temp_dir/checksum"
else
  command -v curl >/dev/null 2>&1 || fail "Required tool not found: curl"
  curl -fSL "$base_url/$archive_name" -o "$temp_dir/$archive_name"
  curl -fSL "$base_url/$archive_name.sha256" -o "$temp_dir/checksum"
fi
expected=$(awk -v name="$archive_name" '$2 == name && length($1) == 64 && $1 ~ /^[a-fA-F0-9]+$/ {print tolower($1)}' "$temp_dir/checksum")
[ "${#expected}" -eq 64 ] || fail "Missing or ambiguous release checksum"
if [ "$hash_tool" = sha256sum ]; then
  actual=$(sha256sum "$temp_dir/$archive_name" | awk '{print $1}')
else
  actual=$(shasum -a 256 "$temp_dir/$archive_name" | awk '{print $1}')
fi
[ "$actual" = "$expected" ] || fail "Release archive checksum mismatch"
tar -xzf "$temp_dir/$archive_name" -C "$temp_dir"
package_root="$temp_dir/graphit-$platform"
for file in graphit graphit-model-server graphit-runtime/LICENSE graphit-runtime/build.json; do
  [ -s "$package_root/$file" ] || fail "Release archive is missing $file"
done
if [ "$platform" = darwin-arm64 ]; then
  [ -s "$package_root/graphit-runtime/macos-libraries.json" ] || fail "Release archive is missing macOS runtime metadata"
fi
mkdir -p "$install_dir"
# Replace files by rename: running executables and mapped libraries retain their
# existing inode until the process exits. Never truncate a live ELF mapping.
# Publish runtime files before the new executables, preserving unrelated assets.
find "$package_root" -type f ! -path "$package_root/graphit" ! -path "$package_root/graphit-model-server" \
  -exec sh -ec '
    root=$1; destination=$2; shift 2
    for source do
      target="$destination/${source#"$root"/}"
      mkdir -p "$(dirname "$target")"
      pending=$(mktemp "$target.graphit-XXXXXX")
      if ! cp -p "$source" "$pending" || ! mv -f "$pending" "$target"; then
        rm -f "$pending"; exit 1
      fi
    done
  ' sh "$package_root" "$install_dir" {} +
for name in graphit graphit-model-server; do
  pending=$(mktemp "$install_dir/.$name-XXXXXX")
  if ! cp "$package_root/$name" "$pending" || ! chmod 755 "$pending" || ! mv -f "$pending" "$install_dir/$name"; then
    rm -f "$pending"; fail "Could not install $name"
  fi
done
printf 'Installed graphit and graphit-model-server in %s\n' "$install_dir"
case ":$PATH:" in
  *":$install_dir:"*) ;;
  *) printf 'Add %s to your PATH.\n' "$install_dir" ;;
esac
printf 'Documentation: https://github.com/%s/tree/main/docs\n' "$repo"
