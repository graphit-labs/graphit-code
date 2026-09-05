#!/usr/bin/env bash
set -euo pipefail

platform=${1:?platform is required}
project_dir=$(cd "${2:?project directory is required}" && pwd)
lbug_mod=${3:?go-ladybug module directory is required}
ort_cache=${4:?ONNX Runtime cache directory is required}
lbug_ext_cache=${5:?LadybugDB extension cache directory is required}
lancedb_dir=${6:?LanceDB output directory is required}

source "$project_dir/native-deps.env"
cache_root=${NATIVE_BUNDLE_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/graphit/native}
bundle_name="graphit-native-${NATIVE_RECIPE_VERSION}-${platform}"
archive_name="$bundle_name.tar.gz"
release_tag="native-${NATIVE_RECIPE_VERSION}"
archive="$cache_root/$release_tag/$archive_name"
bundle_dir="$cache_root/$release_tag/$bundle_name"

hash_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

verify_files() {
  if command -v sha256sum >/dev/null 2>&1; then
    (cd "$bundle_dir" && sha256sum -c SHA256SUMS)
  else
    (cd "$bundle_dir" && shasum -a 256 -c SHA256SUMS)
  fi
}

case "$platform" in
  linux-amd64)
    archive_sha=$NATIVE_BUNDLE_LINUX_AMD64_SHA256
    lancedb_name=liblancedb_go.so
    ort_dir=onnxruntime-linux-x64-${ORT_VERSION}
    ort_name=libonnxruntime.so
    extension_platform=linux_amd64
    ;;
  darwin-arm64)
    archive_sha=$NATIVE_BUNDLE_DARWIN_ARM64_SHA256
    lancedb_name=liblancedb_go.dylib
    ort_dir=onnxruntime-osx-arm64-${ORT_VERSION}
    ort_name=libonnxruntime.dylib
    extension_platform=osx_arm64
    ;;
  windows-amd64)
    archive_sha=$NATIVE_BUNDLE_WINDOWS_AMD64_SHA256
    lancedb_name=lancedb_go.dll
    ort_dir=onnxruntime-win-x64-${ORT_VERSION}
    ort_name=onnxruntime.dll
    extension_platform=win_amd64
    ;;
  *)
    echo "unsupported native bundle platform: $platform" >&2
    exit 1
    ;;
esac

if [ "$archive_sha" = PENDING ] || [ ${#archive_sha} -ne 64 ]; then
  echo "native bundle checksum is not pinned for $platform" >&2
  exit 1
fi

mkdir -p "$(dirname "$archive")"
if [ ! -f "$archive" ] || [ "$(hash_file "$archive")" != "$archive_sha" ]; then
  url="https://github.com/${NATIVE_BUNDLE_REPOSITORY}/releases/download/${release_tag}/${archive_name}"
  echo "  → Downloading verified native dependencies for $platform…"
  curl -fSL --retry 5 --retry-delay 5 --retry-all-errors --connect-timeout 30 "$url" -o "$archive.tmp"
  actual_sha=$(hash_file "$archive.tmp")
  if [ "$actual_sha" != "$archive_sha" ]; then
    echo "native bundle checksum mismatch for $platform" >&2
    exit 1
  fi
  mv "$archive.tmp" "$archive"
fi

if [ ! -f "$bundle_dir/manifest.json" ]; then
  tar -xzf "$archive" -C "$(dirname "$bundle_dir")"
fi

grep -Fq '"recipe": "'"$NATIVE_RECIPE_VERSION"'"' "$bundle_dir/manifest.json"
grep -Fq '"platform": "'"$platform"'"' "$bundle_dir/manifest.json"
verify_files >/dev/null

mkdir -p "$lancedb_dir" "$lbug_mod/lib" "$ort_cache/$ort_dir/lib" \
  "$lbug_ext_cache/$LBUG_EXT_VERSION/$extension_platform"
cp -L "$bundle_dir/lancedb/$lancedb_name" "$lancedb_dir/$lancedb_name.tmp"
mv "$lancedb_dir/$lancedb_name.tmp" "$lancedb_dir/$lancedb_name"
cp "$bundle_dir/lancedb/lancedb_go_build.sha" "$lancedb_dir/lancedb_go_build.sha"
cp -a "$bundle_dir/ladybug/." "$lbug_mod/lib/"
printf '%s\n' "$LBUG_NATIVE_VERSION" > "$lbug_mod/lib/lbug_native_version"
cp -L "$bundle_dir/onnxruntime/$ort_name" "$ort_cache/$ort_dir/lib/$ort_name"
cp "$bundle_dir/ladybug/httpfs.lbug_extension" \
  "$lbug_ext_cache/$LBUG_EXT_VERSION/$extension_platform/httpfs.lbug_extension"

echo "  ✓ Native dependency recipe $NATIVE_RECIPE_VERSION installed for $platform"
