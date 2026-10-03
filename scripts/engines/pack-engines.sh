#!/usr/bin/env bash
# Builds the downloadable engine packs for the current platform:
#   dist-engines/<engine>-<os>-<arch>.zip   (pandoc, ghostscript, sevenzip)
# The app downloads these on first use; see make-manifest.py for the index file.
#
# Usage: ./pack-engines.sh [pandoc] [ghostscript] [sevenzip]   (default: all)
source "$(dirname "$0")/common.sh"

OUT="$ROOT/dist-engines"
mkdir -p "$OUT"
PLATFORM="$OS-$ARCH"   # matches std::env::consts::{OS,ARCH} in the app: macos-aarch64, windows-x86_64 ...
WANT="${*:-pandoc ghostscript sevenzip}"

zip_pack() { # zip_pack <engine> <staging dir>
  local engine="$1" stage="$2" file="$OUT/$1-$PLATFORM.zip"
  rm -f "$file"
  (cd "$stage" && zip -qry "$file" .)
  log "$engine -> $file ($(du -h "$file" | cut -f1))"
}

sevenz() { # 7-Zip to unpack Windows installers (preinstalled on GitHub Windows runners)
  if command -v 7z >/dev/null; then 7z "$@"; else "/c/Program Files/7-Zip/7z.exe" "$@"; fi
}

# ------------------------------------------------------------------- Pandoc
pack_pandoc() {
  local asset stage="$WORK/pack/pandoc"
  case "$PLATFORM" in
    macos-aarch64) asset="pandoc-$PANDOC_VERSION-arm64-macOS.zip" ;;
    macos-x86_64) asset="pandoc-$PANDOC_VERSION-x86_64-macOS.zip" ;;
    windows-x86_64) asset="pandoc-$PANDOC_VERSION-windows-x86_64.zip" ;;
    *) echo "no pandoc build for $PLATFORM" >&2; return ;;
  esac
  curl -fL --retry 6 --retry-delay 5 --retry-all-errors -o "$SRC/$asset" "https://github.com/jgm/pandoc/releases/download/$PANDOC_VERSION/$asset"
  rm -rf "$stage" "$WORK/pack/pandoc-x" && mkdir -p "$stage" "$WORK/pack/pandoc-x"
  unzip -q "$SRC/$asset" -d "$WORK/pack/pandoc-x"
  cp "$(find "$WORK/pack/pandoc-x" -type f -name "pandoc$EXE" | head -1)" "$stage/"
  cp "$(find "$WORK/pack/pandoc-x" -type f -iname 'COPYING*' | head -1)" "$stage/COPYING.txt" 2>/dev/null || true
  chmod +x "$stage/pandoc$EXE"
  zip_pack pandoc "$stage"
}

# -------------------------------------------------------------- Ghostscript
pack_ghostscript() {
  local tag="gs${GHOSTSCRIPT_VERSION//./}" stage="$WORK/pack/ghostscript"
  local base="https://github.com/ArtifexSoftware/ghostpdl-downloads/releases/download/$tag"
  rm -rf "$stage" && mkdir -p "$stage/bin"
  if [ "$OS" = windows ]; then
    # The official installer is an NSIS archive; 7-Zip can unpack it without running it.
    curl -fL --retry 6 --retry-delay 5 --retry-all-errors -o "$SRC/gs-win.exe" "$base/${tag}w64.exe"
    rm -rf "$WORK/pack/gs-x" && sevenz x -y -o"$(cygpath -w "$WORK/pack/gs-x")" "$(cygpath -w "$SRC/gs-win.exe")" >/dev/null
    cp "$WORK/pack/gs-x/bin/gswin64c.exe" "$WORK/pack/gs-x/bin/gsdll64.dll" "$stage/bin/"
    cp "$WORK/pack/gs-x/doc/COPYING" "$stage/COPYING.txt" 2>/dev/null || true
  else
    # Build from source: one self-contained `gs` with its resources compiled in.
    local d
    d="$(fetch "$base/ghostscript-$GHOSTSCRIPT_VERSION.tar.gz")"
    if ! done_before ghostscript; then
      log "Building Ghostscript"
      (
        cd "$d"
        # Ghostscript's bundled (old) zlib turns fdopen() into a macro on macOS, which breaks
        # against the macOS 15 SDK's <stdio.h>. Remove that one line; newer zlib does the same.
        perl -pi -e 's/^\s*#\s*define fdopen\(fd,mode\) NULL.*$//' zlib/zutil.h
        # Same story for its bundled libpng: TARGET_OS_MAC makes it include the long-gone <fp.h>.
        perl -pi -e 's/\|\|\s*defined\(TARGET_OS_MAC\)//' libpng/pngpriv.h
        # Ghostscript ships its own copies of libpng/jpeg/zlib/freetype/lcms; don't let it see ours.
        PKG_CONFIG_PATH= PKG_CONFIG_LIBDIR=/nonexistent CPPFLAGS= LDFLAGS= \
          ./configure --without-x --disable-cups --disable-gtk --disable-dbus --disable-fontconfig \
          --without-libidn --without-libpaper --without-tesseract --without-ijs
        make -j "$JOBS"
      ) >&2
      mark_done ghostscript
    fi
    cp "$d/bin/gs" "$stage/bin/gs"
    strip -x "$stage/bin/gs" 2>/dev/null || strip "$stage/bin/gs" || true
    if [ "$OS" = macos ] && otool -L "$stage/bin/gs" | grep -qE '/opt/homebrew|/usr/local/'; then
      echo "ERROR: gs links a Homebrew library:" >&2; otool -L "$stage/bin/gs" >&2; exit 1
    fi
    cp "$d/LICENSE" "$stage/COPYING.txt" 2>/dev/null || true
  fi
  zip_pack ghostscript "$stage"
}

# ------------------------------------------------------------------- 7-Zip
pack_sevenzip() {
  local v="${SEVENZIP_VERSION//./}" stage="$WORK/pack/sevenzip"
  local base="https://github.com/ip7z/7zip/releases/download/$SEVENZIP_VERSION"
  rm -rf "$stage" && mkdir -p "$stage"
  if [ "$OS" = windows ]; then
    # Full 7z.exe + 7z.dll (supports RAR); unpacked from the official installer.
    curl -fL --retry 6 --retry-delay 5 --retry-all-errors -o "$SRC/7z-win.exe" "$base/7z$v-x64.exe"
    rm -rf "$WORK/pack/7z-x" && sevenz x -y -o"$(cygpath -w "$WORK/pack/7z-x")" "$(cygpath -w "$SRC/7z-win.exe")" >/dev/null
    cp "$WORK/pack/7z-x/7z.exe" "$WORK/pack/7z-x/7z.dll" "$stage/"
    cp "$WORK/pack/7z-x/License.txt" "$stage/License.txt"
  else
    # 7zz is a universal (arm64 + x86_64) static binary.
    curl -fL --retry 6 --retry-delay 5 --retry-all-errors -o "$SRC/7z-mac.tar.xz" "$base/7z$v-mac.tar.xz"
    tar -xf "$SRC/7z-mac.tar.xz" -C "$stage" 7zz License.txt
    chmod +x "$stage/7zz"
  fi
  zip_pack sevenzip "$stage"
}

for e in $WANT; do
  "pack_$e"
done
ls -la "$OUT"
