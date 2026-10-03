#!/usr/bin/env bash
# Builds a slim, static FFmpeg for Morfyl and installs it as a Tauri sidecar:
#   src-tauri/binaries/ffmpeg-<target-triple>[.exe]
#
# Default is an LGPL build. H.264/HEVC encoding uses the OS encoders
# (VideoToolbox on macOS, Media Foundation on Windows), so no GPL x264 is needed.
#   GPL=1 ./build-ffmpeg.sh    adds libx264 (better quality per MB; makes the binary GPL)
#
# Requirements: macOS -> `brew install nasm meson ninja cmake autoconf automake libtool pkg-config`
#               Windows -> MSYS2 UCRT64 with mingw-w64-ucrt-x86_64-{toolchain,nasm,meson,ninja,cmake,pkgconf} + autotools
source "$(dirname "$0")/common.sh"
GPL="${GPL:-0}"

build_zlib

# ---- MP3 encoder
d="$(fetch "https://downloads.sourceforge.net/project/lame/lame/$LAME_VERSION/lame-$LAME_VERSION.tar.gz")"
LAME_ASM=(); [ "$ARCH" = x86_64 ] && LAME_ASM=(--enable-nasm)
autotools_build lame "$d" --disable-frontend --disable-decoder --disable-gtktest "${LAME_ASM[@]}"
license "$d/COPYING" lame

# ---- Opus
d="$(fetch "https://downloads.xiph.org/releases/opus/opus-$OPUS_VERSION.tar.gz")"
cmake_build opus "$d" -DOPUS_BUILD_PROGRAMS=OFF -DOPUS_BUILD_TESTING=OFF -DOPUS_INSTALL_PKG_CONFIG_MODULE=ON
license "$d/COPYING" opus

# ---- Ogg + Vorbis
d="$(fetch "https://downloads.xiph.org/releases/ogg/libogg-$OGG_VERSION.tar.gz")"
cmake_build ogg "$d" -DINSTALL_DOCS=OFF -DBUILD_TESTING=OFF
d="$(fetch "https://downloads.xiph.org/releases/vorbis/libvorbis-$VORBIS_VERSION.tar.gz")"
cmake_build vorbis "$d"
license "$d/COPYING" libvorbis

# ---- VP8/VP9 (WebM)
d="$(fetch "https://github.com/webmproject/libvpx/archive/refs/tags/v$LIBVPX_VERSION.tar.gz" "libvpx-$LIBVPX_VERSION")"
if ! done_before libvpx; then
  log "Building libvpx"
  case "$OS-$ARCH" in
    macos-aarch64) VPX_TARGET=arm64-darwin20-gcc ;;
    macos-x86_64) VPX_TARGET=x86_64-darwin20-gcc ;;
    windows-x86_64) VPX_TARGET=x86_64-win64-gcc ;;
    *) VPX_TARGET=generic-gnu ;;
  esac
  (
    cd "$d"
    ./configure --prefix="$PREFIX" --target="$VPX_TARGET" --enable-static --disable-shared --enable-pic \
      --disable-examples --disable-tools --disable-docs --disable-unit-tests --enable-vp9-highbitdepth \
      $([ "$ARCH" = x86_64 ] && echo --as=nasm)
    make -j "$JOBS"
    make install
  ) >&2
  mark_done libvpx
fi
license "$d/LICENSE" libvpx

# ---- AV1 decoding
d="$(fetch "https://downloads.videolan.org/pub/videolan/dav1d/$DAV1D_VERSION/dav1d-$DAV1D_VERSION.tar.xz")"
meson_build dav1d "$d" -Denable_tools=false -Denable_tests=false -Denable_examples=false
license "$d/COPYING" dav1d

# ---- optional x264 (GPL)
if [ "$GPL" = 1 ]; then
  d="$(fetch "https://code.videolan.org/videolan/x264/-/archive/stable/x264-stable.tar.bz2")"
  autotools_build x264 "$d" --disable-cli --enable-pic --disable-opencl
fi

# ---- FFmpeg
d="$(fetch "https://github.com/FFmpeg/FFmpeg/archive/refs/tags/n$FFMPEG_VERSION.tar.gz" "FFmpeg-n$FFMPEG_VERSION")"

ENCODERS="mpeg4,gif,png,mjpeg,libvpx_vp8,libvpx_vp9,aac,ac3,libmp3lame,libopus,opus,libvorbis,vorbis,flac,alac,pcm_s16le,pcm_s16be,pcm_s24le,pcm_f32le"
MUXERS="mp4,mov,ipod,matroska,webm,avi,gif,mp3,adts,wav,aiff,flac,ogg,opus,image2,null"
FILTERS="buffer,buffersink,abuffer,abuffersink,format,aformat,null,anull,scale,fps,split,asplit,palettegen,paletteuse,aresample,setsar,setpts,asetpts,transpose,hflip,vflip,crop,pad,trim,atrim,volume,copy"
EXTRA=()
case "$OS" in
  macos)
    ENCODERS="$ENCODERS,h264_videotoolbox,hevc_videotoolbox,aac_at"
    EXTRA+=(--enable-videotoolbox --enable-audiotoolbox)
    ;;
  windows)
    ENCODERS="$ENCODERS,h264_mf,hevc_mf"
    EXTRA+=(--enable-mediafoundation --extra-ldflags=-static)
    ;;
esac
if [ "$GPL" = 1 ]; then
  ENCODERS="$ENCODERS,libx264"
  EXTRA+=(--enable-gpl --enable-libx264)
fi

if ! done_before ffmpeg; then
  log "Configuring FFmpeg ($([ "$GPL" = 1 ] && echo GPL || echo LGPL))"
  (
    cd "$d"
    # Keep every decoder, demuxer and parser (opening anything is the point of a converter),
    # but only the encoders, muxers and filters Morfyl actually uses.
    ./configure --prefix="$PREFIX" \
      --pkg-config-flags=--static \
      --extra-cflags="-I$PREFIX/include $CFLAGS" \
      --extra-ldflags="-L$PREFIX/lib $LDFLAGS" \
      --extra-libs="-lm" \
      --enable-static --disable-shared \
      --disable-autodetect --disable-debug --disable-doc --disable-network \
      --disable-ffplay --disable-ffprobe --disable-devices --disable-indevs --disable-outdevs \
      --disable-encoders --enable-encoder="$ENCODERS" \
      --disable-muxers --enable-muxer="$MUXERS" \
      --disable-filters --enable-filter="$FILTERS" \
      --disable-protocols --enable-protocol=file,pipe \
      --enable-zlib --enable-libmp3lame --enable-libopus --enable-libvorbis --enable-libvpx --enable-libdav1d \
      --enable-small \
      "${EXTRA[@]}"
    make -j "$JOBS"
  ) >&2
  mark_done ffmpeg
fi

"$d/ffmpeg$EXE" -hide_banner -encoders | grep -E 'libvpx-vp9|libmp3lame|libopus|h264_' || true
install_sidecar "$d/ffmpeg$EXE" ffmpeg
if [ "$GPL" = 1 ]; then license "$d/COPYING.GPLv2" ffmpeg; else license "$d/COPYING.LGPLv2.1" ffmpeg; fi
