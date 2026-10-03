Third-party engines used by Morfyl
=====================================

Morfyl runs these programs as separate processes. It does not link to them.
Their license texts are copied into this folder by the release build
(scripts/engines/*.sh put each engine's COPYING/LICENSE file here).

Bundled in the installer
  ImageMagick  - ImageMagick License (Apache 2.0 style)   https://imagemagick.org/script/license.php
  FFmpeg       - LGPL 2.1 or later (GPL if built with GPL=1) https://ffmpeg.org/legal.html
                 Source: https://github.com/FFmpeg/FFmpeg (exact tag in scripts/engines/versions.env)
  Libraries compiled into those two: libjpeg-turbo (BSD/IJG), libpng (libpng), zlib (zlib),
  libwebp (BSD), libtiff (libtiff), lcms2 (MIT), libheif (LGPL 3), libde265 (LGPL 3),
  libaom (BSD 2), openjpeg (BSD 2), LibRaw (LGPL 2.1 / CDDL), lame (LGPL 2),
  opus (BSD 3), libogg + libvorbis (BSD 3), libvpx (BSD 3)

Downloaded on first use
  Pandoc       - GPL 2 or later        https://github.com/jgm/pandoc
  Ghostscript  - AGPL 3                https://ghostscript.com  (commercial licence needed for closed-source redistribution)
  7-Zip        - LGPL 2.1 + unRAR      https://www.7-zip.org

Used if installed by the user (never redistributed)
  LibreOffice  - MPL 2.0
  Calibre      - GPL 3
