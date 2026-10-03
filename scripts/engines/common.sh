#!/usr/bin/env bash
# Shared helpers for the engine build scripts.
# Works on macOS (Xcode CLT + Homebrew build tools) and Windows (MSYS2 UCRT64 shell).
set -eo pipefail  # (no -u: macOS bash 3.2 treats empty arrays as unbound)

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
# shellcheck source=versions.env
source "$HERE/versions.env"

WORK="${WORK:-$ROOT/.engine-build}"
PREFIX="$WORK/prefix"
SRC="$WORK/src"
STAMPS="$WORK/stamps"
BIN_OUT="$ROOT/src-tauri/binaries"
LICENSE_OUT="$ROOT/LICENSES"
mkdir -p "$PREFIX/lib/pkgconfig" "$PREFIX/include" "$SRC" "$STAMPS" "$BIN_OUT" "$LICENSE_OUT"

JOBS="$(getconf _NPROCESSORS_ONLN 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)"

case "$(uname -s)" in
  Darwin) OS=macos ;;
  MINGW* | MSYS* | CYGWIN*) OS=windows ;;
  Linux) OS=linux ;;
  *) echo "Unsupported OS $(uname -s)" >&2; exit 1 ;;
esac
ARCH="$(uname -m)"
[ "$ARCH" = "arm64" ] && ARCH=aarch64
EXE=""
[ "$OS" = windows ] && EXE=".exe"

# Tauri names sidecars <name>-<rust target triple>. On Windows the app is built with MSVC Rust
# even though the engines are compiled with MinGW, so CI passes TARGET_TRIPLE explicitly.
if [ -z "${TARGET_TRIPLE:-}" ]; then
  TARGET_TRIPLE="$(rustc -vV 2>/dev/null | sed -n 's/^host: //p' || true)"
fi
if [ -z "$TARGET_TRIPLE" ]; then
  case "$OS-$ARCH" in
    macos-aarch64) TARGET_TRIPLE=aarch64-apple-darwin ;;
    macos-x86_64) TARGET_TRIPLE=x86_64-apple-darwin ;;
    windows-x86_64) TARGET_TRIPLE=x86_64-pc-windows-msvc ;;
    linux-x86_64) TARGET_TRIPLE=x86_64-unknown-linux-gnu ;;
  esac
fi

# Only ever see our own static libraries, never Homebrew's or the system's dylibs.
export PKG_CONFIG_PATH="$PREFIX/lib/pkgconfig"
export PKG_CONFIG_LIBDIR="$PREFIX/lib/pkgconfig"
export PATH="$PREFIX/bin:$PATH"

OPT_FLAGS="-Os -fPIC -ffunction-sections -fdata-sections"
export CFLAGS="$OPT_FLAGS ${EXTRA_CFLAGS:-}"
export CXXFLAGS="$OPT_FLAGS ${EXTRA_CFLAGS:-}"
export CPPFLAGS="-I$PREFIX/include"
export LDFLAGS="-L$PREFIX/lib"
CXX_RUNTIME="-lstdc++"

CMAKE_COMMON=(
  -G Ninja
  -DCMAKE_BUILD_TYPE=MinSizeRel
  -DCMAKE_INSTALL_PREFIX="$PREFIX"
  -DCMAKE_PREFIX_PATH="$PREFIX"
  -DCMAKE_INSTALL_LIBDIR=lib
  -DBUILD_SHARED_LIBS=OFF
  -DCMAKE_POSITION_INDEPENDENT_CODE=ON
  -DCMAKE_POLICY_VERSION_MINIMUM=3.5
)

if [ "$OS" = macos ]; then
  export MACOSX_DEPLOYMENT_TARGET=11.0
  export CFLAGS="$CFLAGS -mmacosx-version-min=11.0"
  export CXXFLAGS="$CXXFLAGS -mmacosx-version-min=11.0"
  export LDFLAGS="$LDFLAGS -mmacosx-version-min=11.0 -Wl,-dead_strip"
  CMAKE_COMMON+=(-DCMAKE_OSX_DEPLOYMENT_TARGET=11.0)
  CXX_RUNTIME="-lc++"
elif [ "$OS" = windows ]; then
  # Fully static .exe: no MinGW DLLs to ship next to it.
  export LDFLAGS="$LDFLAGS -static -static-libgcc -static-libstdc++ -Wl,--gc-sections"
else
  export LDFLAGS="$LDFLAGS -Wl,--gc-sections"
fi

log() { printf '\n\033[1;33m==> %s\033[0m\n' "$*"; }

done_before() { [ -f "$STAMPS/$1" ]; }
mark_done() { touch "$STAMPS/$1"; }

# fetch <url> [dirname] -> prints the extracted source directory
fetch() {
  # Runs inside $(...), where `set -e` does not apply, so every step checks for failure itself.
  local url="$1" name="${2:-}" file top
  file="$SRC/$(basename "${url%%\?*}")"
  if [ ! -f "$file" ]; then
    if ! curl -fL --retry 6 --retry-delay 5 --retry-all-errors --connect-timeout 30 \
      -o "$file.part" "$url" >&2; then
      echo "ERROR: download failed: $url" >&2
      rm -f "$file.part"
      return 1
    fi
    mv "$file.part" "$file" || return 1
  fi
  top="$(tar -tf "$file" 2>/dev/null | head -1 | cut -d/ -f1)"
  if [ -z "$top" ]; then
    echo "ERROR: $file is not a valid archive" >&2
    rm -f "$file"
    return 1
  fi
  [ -n "$name" ] || name="$top"
  if [ ! -d "$SRC/$name" ]; then
    tar -xf "$file" -C "$SRC" >&2 || return 1
    if [ "$top" != "$name" ]; then mv "$SRC/$top" "$SRC/$name" || return 1; fi
  fi
  echo "$SRC/$name"
}

# cmake_build <stamp> <srcdir> [cmake args...]
cmake_build() {
  local stamp="$1" dir="$2"
  shift 2
  done_before "$stamp" && return 0
  log "Building $stamp"
  rm -rf "$dir/_ff_build"
  cmake -S "$dir" -B "$dir/_ff_build" "${CMAKE_COMMON[@]}" "$@" >&2
  cmake --build "$dir/_ff_build" -j "$JOBS" >&2
  cmake --install "$dir/_ff_build" >&2
  mark_done "$stamp"
}

# autotools_build <stamp> <srcdir> [configure args...]
autotools_build() {
  local stamp="$1" dir="$2"
  shift 2
  done_before "$stamp" && return 0
  log "Building $stamp"
  (
    cd "$dir"
    [ -x configure ] || autoreconf -fi
    ./configure --prefix="$PREFIX" --libdir="$PREFIX/lib" --enable-static --disable-shared "$@"
    make -j "$JOBS"
    make install
  ) >&2
  mark_done "$stamp"
}

# meson_build <stamp> <srcdir> [meson args...]
meson_build() {
  local stamp="$1" dir="$2"
  shift 2
  done_before "$stamp" && return 0
  log "Building $stamp"
  rm -rf "$dir/_ff_build"
  meson setup "$dir/_ff_build" "$dir" --prefix="$PREFIX" --libdir=lib --buildtype=minsize \
    --default-library=static "$@" >&2
  meson compile -C "$dir/_ff_build" >&2
  meson install -C "$dir/_ff_build" >&2
  mark_done "$stamp"
}

license() { # license <file> <name>
  if [ -f "$1" ]; then cp "$1" "$LICENSE_OUT/$2.txt"; else echo "warning: no license file $1" >&2; fi
}

# install_sidecar <built binary> <name>
install_sidecar() {
  local src="$1" name="$2" dest
  dest="$BIN_OUT/$name-$TARGET_TRIPLE$EXE"
  cp "$src" "$dest"
  if [ "$OS" = macos ]; then strip -x "$dest"; else strip "$dest" 2>/dev/null || true; fi
  chmod +x "$dest"
  log "$name -> $dest ($(du -h "$dest" | cut -f1))"
  if [ "$OS" = macos ]; then
    echo "Linked libraries (must be system ones only):"
    otool -L "$dest" | tail -n +2
    if otool -L "$dest" | grep -qE '/opt/homebrew|/usr/local/(opt|Cellar|lib)|\.engine-build'; then
      echo "ERROR: $name links a non-system dylib; it would not run on other Macs." >&2
      exit 1
    fi
  elif [ "$OS" = windows ]; then
    echo "DLL imports (must be Windows system DLLs only):"
    objdump -p "$dest" | grep 'DLL Name' || true
    if objdump -p "$dest" | grep -iqE 'DLL Name: (lib|zlib1|libwinpthread|libgcc|libstdc)'; then
      echo "ERROR: $name depends on a MinGW DLL; it would not run without MSYS2." >&2
      exit 1
    fi
  fi
}

# --------------------------------------------------------------- shared deps

build_zlib() {
  local d
  d="$(fetch "https://github.com/madler/zlib/releases/download/v$ZLIB_VERSION/zlib-$ZLIB_VERSION.tar.gz")"
  cmake_build zlib "$d" -DZLIB_BUILD_EXAMPLES=OFF
  # zlib's CMake always builds a shared lib too; remove it so nothing links it.
  rm -f "$PREFIX"/lib/libz*.dylib "$PREFIX"/lib/libz*.so* "$PREFIX"/lib/libzlib.dll.a "$PREFIX"/bin/libzlib*.dll
  [ -f "$PREFIX/lib/libz.a" ] || cp "$PREFIX/lib/libzlibstatic.a" "$PREFIX/lib/libz.a" 2>/dev/null || true
  license "$d/LICENSE" zlib
}

build_aom() {
  local d
  d="$(fetch "https://storage.googleapis.com/aom-releases/libaom-$LIBAOM_VERSION.tar.gz")"
  # libaom's NASM check parses `nasm -hO` help text, which NASM 3.x reworded, so it wrongly
  # rejects a working assembler. Downgrade that check from fatal to a warning.
  perl -0777 -pi -e 's/FATAL_ERROR(\s*"Unsupported nasm)/WARNING$1/g' \
    "$d/build/cmake/aom_optimization.cmake"
  cmake_build aom "$d" -DENABLE_DOCS=0 -DENABLE_EXAMPLES=0 -DENABLE_TESTS=0 -DENABLE_TOOLS=0 \
    -DENABLE_TESTDATA=0 -DCONFIG_RUNTIME_CPU_DETECT=1
  license "$d/LICENSE" libaom
}
