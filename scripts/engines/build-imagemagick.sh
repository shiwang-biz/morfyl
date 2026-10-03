#!/usr/bin/env bash
# Builds a slim, static ImageMagick 7 (`magick`) for Morfyl and installs it as a Tauri sidecar:
#   src-tauri/binaries/magick-<target-triple>[.exe]
#
# Included: JPEG, PNG, WebP, TIFF, GIF, BMP, ICO, PSD, TGA, AVIF (read+write), HEIC (read),
#           JPEG 2000, camera RAW (LibRaw), SVG (built-in renderer), colour management (lcms2).
# Left out to stay small: X11, fonts/text rendering, OpenEXR, DjVu, FFTW, Ghostscript PDF reading
# (Morfyl reads PDFs with Ghostscript directly).
#   WITH_JXL=1 ./build-imagemagick.sh   adds JPEG XL (+~3 MB)
#
# Same requirements as build-ffmpeg.sh.
source "$(dirname "$0")/common.sh"
WITH_JXL="${WITH_JXL:-0}"

build_zlib

d="$(fetch "https://github.com/libjpeg-turbo/libjpeg-turbo/releases/download/$LIBJPEG_TURBO_VERSION/libjpeg-turbo-$LIBJPEG_TURBO_VERSION.tar.gz")"
cmake_build libjpeg-turbo "$d" -DENABLE_SHARED=OFF -DENABLE_STATIC=ON -DWITH_TURBOJPEG=OFF -DWITH_JPEG8=ON
license "$d/LICENSE.md" libjpeg-turbo

d="$(fetch "https://download.sourceforge.net/libpng/libpng-$LIBPNG_VERSION.tar.xz")"
cmake_build libpng "$d" -DPNG_SHARED=OFF -DPNG_STATIC=ON -DPNG_TESTS=OFF -DPNG_TOOLS=OFF -DPNG_FRAMEWORK=OFF
license "$d/LICENSE" libpng

d="$(fetch "https://storage.googleapis.com/downloads.webmproject.org/releases/webp/libwebp-$LIBWEBP_VERSION.tar.gz")"
cmake_build libwebp "$d" -DWEBP_BUILD_ANIM_UTILS=OFF -DWEBP_BUILD_CWEBP=OFF -DWEBP_BUILD_DWEBP=OFF \
  -DWEBP_BUILD_GIF2WEBP=OFF -DWEBP_BUILD_IMG2WEBP=OFF -DWEBP_BUILD_VWEBP=OFF -DWEBP_BUILD_WEBPINFO=OFF \
  -DWEBP_BUILD_WEBPMUX=OFF -DWEBP_BUILD_EXTRAS=OFF -DWEBP_BUILD_LIBWEBPMUX=ON
license "$d/COPYING" libwebp

d="$(fetch "https://download.osgeo.org/libtiff/tiff-$LIBTIFF_VERSION.tar.gz")"
cmake_build libtiff "$d" -Dtiff-tools=OFF -Dtiff-tests=OFF -Dtiff-docs=OFF -Dtiff-contrib=OFF \
  -Djbig=OFF -Dlerc=OFF -Dlzma=OFF -Dzstd=OFF -Dwebp=OFF -Dlibdeflate=OFF -Dcxx=OFF
license "$d/LICENSE.md" libtiff

d="$(fetch "https://github.com/mm2/Little-CMS/releases/download/lcms$LCMS2_VERSION/lcms2-$LCMS2_VERSION.tar.gz")"
autotools_build lcms2 "$d" --without-jpeg --without-tiff
license "$d/LICENSE" lcms2

d="$(fetch "https://github.com/uclouvain/openjpeg/archive/refs/tags/v$OPENJPEG_VERSION.tar.gz" "openjpeg-$OPENJPEG_VERSION")"
cmake_build openjpeg "$d" -DBUILD_CODEC=OFF -DBUILD_TESTING=OFF
license "$d/LICENSE" openjpeg

d="$(fetch "https://github.com/strukturag/libde265/releases/download/v$LIBDE265_VERSION/libde265-$LIBDE265_VERSION.tar.gz")"
cmake_build libde265 "$d" -DENABLE_SDL=OFF -DENABLE_DECODER=OFF -DENABLE_ENCODER=OFF
license "$d/COPYING" libde265

build_aom

d="$(fetch "https://github.com/strukturag/libheif/releases/download/v$LIBHEIF_VERSION/libheif-$LIBHEIF_VERSION.tar.gz")"
cmake_build libheif "$d" -DWITH_LIBDE265=ON -DWITH_AOM_DECODER=ON -DWITH_AOM_ENCODER=ON \
  -DWITH_X265=OFF -DWITH_DAV1D=OFF -DWITH_RAV1E=OFF -DWITH_SvtEnc=OFF -DWITH_KVAZAAR=OFF \
  -DWITH_JPEG_DECODER=OFF -DWITH_JPEG_ENCODER=OFF -DWITH_OpenJPEG_DECODER=OFF -DWITH_OpenJPEG_ENCODER=OFF \
  -DWITH_FFMPEG_DECODER=OFF -DWITH_LIBSHARPYUV=OFF -DWITH_EXAMPLES=OFF -DWITH_GDK_PIXBUF=OFF \
  -DENABLE_PLUGIN_LOADING=OFF -DBUILD_TESTING=OFF
license "$d/COPYING" libheif

d="$(fetch "https://download.gnome.org/sources/libxml2/${LIBXML2_VERSION%.*}/libxml2-$LIBXML2_VERSION.tar.xz")"
cmake_build libxml2 "$d" -DLIBXML2_WITH_PYTHON=OFF -DLIBXML2_WITH_ICONV=OFF -DLIBXML2_WITH_ICU=OFF \
  -DLIBXML2_WITH_LZMA=OFF -DLIBXML2_WITH_ZLIB=OFF -DLIBXML2_WITH_TESTS=OFF -DLIBXML2_WITH_PROGRAMS=OFF \
  -DLIBXML2_WITH_HTTP=OFF -DLIBXML2_WITH_FTP=OFF
license "$d/Copyright" libxml2

d="$(fetch "https://www.libraw.org/data/LibRaw-$LIBRAW_VERSION.tar.gz")"
autotools_build libraw "$d" --disable-examples --disable-openmp --disable-jasper --disable-lcms \
  CPPFLAGS="$CPPFLAGS -DLIBRAW_NODLL"
license "$d/LICENSE.LGPL" libraw

if [ "$WITH_JXL" = 1 ] && ! done_before libjxl; then
  log "Building libjxl"
  [ -d "$SRC/libjxl" ] || git clone --depth 1 --branch "v$LIBJXL_VERSION" --recursive --shallow-submodules \
    https://github.com/libjxl/libjxl.git "$SRC/libjxl" >&2
  cmake_build libjxl "$SRC/libjxl" -DBUILD_TESTING=OFF -DJPEGXL_ENABLE_TOOLS=OFF -DJPEGXL_ENABLE_DOXYGEN=OFF \
    -DJPEGXL_ENABLE_MANPAGES=OFF -DJPEGXL_ENABLE_BENCHMARK=OFF -DJPEGXL_ENABLE_EXAMPLES=OFF \
    -DJPEGXL_ENABLE_JNI=OFF -DJPEGXL_ENABLE_SJPEG=OFF -DJPEGXL_ENABLE_OPENEXR=OFF -DJPEGXL_ENABLE_SKCMS=ON \
    -DJPEGXL_BUNDLE_LIBPNG=OFF -DJPEGXL_ENABLE_JPEGLI=OFF -DJPEGXL_STATIC=ON
  license "$SRC/libjxl/LICENSE" libjxl
fi

# ---- ImageMagick
d="$(fetch "https://github.com/ImageMagick/ImageMagick/archive/refs/tags/$IMAGEMAGICK_VERSION.tar.gz" "ImageMagick-$IMAGEMAGICK_VERSION")"
if ! done_before imagemagick; then
  log "Configuring ImageMagick"
  (
    cd "$d"
    # pkg-config --static pulls in each library's private deps (libheif -> aom, de265, C++ runtime).
    PKG_CONFIG="pkg-config --static" ./configure --prefix="$PREFIX" \
      --enable-static --disable-shared --without-modules --disable-installed \
      --with-quantum-depth=16 --disable-hdri --disable-openmp --disable-opencl --disable-docs \
      --without-magick-plus-plus --without-perl --without-x \
      --without-fontconfig --without-freetype --without-raqm --without-pango \
      --without-djvu --without-fftw --without-gslib --without-gvc --without-lqr --without-openexr \
      --without-wmf --without-zstd --without-lzma --without-bzlib --without-jbig --without-dps \
      --without-flif --without-zip --without-uhdr \
      --with-zlib --with-jpeg --with-png --with-webp --with-tiff --with-lcms --with-openjp2 \
      --with-heic --with-raw --with-xml \
      $([ "$WITH_JXL" = 1 ] && echo --with-jxl || echo --without-jxl) \
      LIBS="$CXX_RUNTIME -lm"
    make -j "$JOBS"
    make install
  ) >&2
  mark_done imagemagick
fi

"$PREFIX/bin/magick$EXE" -version
echo "Writable formats:"
"$PREFIX/bin/magick$EXE" -list format | awk '$3 ~ /w/ {print $1}' | tr '\n' ' '
echo
install_sidecar "$PREFIX/bin/magick$EXE" magick
license "$d/LICENSE" imagemagick
