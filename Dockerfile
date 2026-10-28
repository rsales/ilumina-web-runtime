# iLumina Web Runtime
#
# EXPERIMENTAL BRANCH: Wine 11 new-WoW64 (see ADR 0006).
#
# Runs the original Windows build of iLumina Gold (Macromedia Director MX
# projector + Xtras + QuickTime 6.3) under Wine, and streams it to the
# browser with the Xpra HTML5 client (video + audio).
#
# This image contains NO iLumina content. The original CDs are mounted at
# runtime (see compose.yaml) and installed into a persistent Wine prefix on
# first start — the same "bring your own copy" model used by nfsu2-web.
#
# Everything comes from the Ubuntu archive so the build is reproducible
# without third-party repositories.

FROM ubuntu:24.04

ARG DEBIAN_FRONTEND=noninteractive

# EXPERIMENT (branch experiment/wine11-wow64):
# Wine 11 built in pure "new WoW64" mode: 32-bit Windows code runs inside a
# 64-bit Linux process, and there are no i386 Linux binaries at all. On Apple
# Silicon Docker then runs everything through Rosetta (x86_64) instead of QEMU
# user-mode (i386), which broke QuickTimeInstaller.exe with status c0000018.
#
# WineHQ's Ubuntu packages are NOT a pure WoW64 build (they still ship
# i386-unix and need it for 32-bit programs: tested 2026-10-28), so we use
# Kron4ek's prebuilt "amd64-wow64" tarball, pinned by SHA-256.
# See docs/decisions/0006-experiment-wine11-wow64.md.
ARG WINE_VERSION=11.0
ARG WINE_SHA256=39574efa1132c3ca0d5c77dd2eddbe4a49cca0d6cc2c290ff4924493a1c40314
ADD --checksum=sha256:${WINE_SHA256} \
    https://github.com/Kron4ek/Wine-Builds/releases/download/${WINE_VERSION}/wine-${WINE_VERSION}-amd64-wow64.tar.xz \
    /tmp/wine.tar.xz

# No "dpkg --add-architecture i386": nothing 32-bit on the Linux side.
# Ubuntu's wine64 package is installed only to pull in the 64-bit runtime
# libraries Wine needs; the Kron4ek build in /opt/wine comes first in PATH.
RUN printf '%s\n' \
      'Types: deb' \
      'URIs: http://archive.ubuntu.com/ubuntu' \
      'Suites: noble noble-updates noble-security' \
      'Components: main restricted universe multiverse' \
      'Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg' \
      > /etc/apt/sources.list.d/ubuntu.sources \
 && rm -f /etc/apt/sources.list \
 && apt-get update \
 && apt-get install -y --no-install-recommends \
      ca-certificates xz-utils tini procps file \
      wine64 libwine libgnutls30t64 libpulse0 libasound2-plugins \
      libgstreamer1.0-0 libgstreamer-plugins-base1.0-0 \
      xvfb xauth x11-utils x11-xserver-utils xdotool imagemagick \
      xpra libjs-jquery python3-pil python3-numpy python3-lz4 python3-xdg \
      python3-pyinotify python3-gi python3-gi-cairo python3-cairo gir1.2-gtk-3.0 \
      python3-gst-1.0 gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
      gstreamer1.0-pulseaudio gstreamer1.0-tools \
      pulseaudio pulseaudio-utils \
      fonts-liberation fonts-dejavu-core \
 && sed -i 's|^start = /etc/X11/Xsession true|# start = (disabled: no desktop session needed)|' \
      /etc/xpra/conf.d/60_server.conf \
 && mkdir -p /opt/wine && tar -C /opt/wine --strip-components=1 -xJf /tmp/wine.tar.xz \
 && rm -f /tmp/wine.tar.xz && rm -rf /var/lib/apt/lists/* \
 && W=/opt/wine/lib/wine && echo "Wine layout:" && ls "$W" \
 && test ! -d "$W/i386-unix" && test -d "$W/i386-windows" \
 && test -f "$W/x86_64-windows/wow64.dll" \
 && /opt/wine/bin/wine --version

# Wine derives the Windows locale from LANG; C.UTF-8 maps to en-US with the
# cp1252 ANSI codepage, which renders the Portuguese text correctly.
ENV LANG=C.UTF-8 \
    LC_ALL=C.UTF-8 \
    PATH=/opt/wine/bin:$PATH \
    WINEARCH=win64 \
    WINEPREFIX=/state/prefix \
    WINEDEBUG=-all \
    ILUMINA_MEDIA=/media/ilumina \
    ILUMINA_RES=1024x768 \
    XPRA_PORT=14500

# The official ubuntu:24.04 image already ships a "ubuntu" user with UID 1000.
RUN (userdel -r ubuntu 2>/dev/null || true) \
 && useradd --create-home --uid 1000 --shell /bin/bash ilumina \
 && mkdir -p /state /media/ilumina \
 && chown ilumina:ilumina /state

COPY runtime/ /opt/ilumina/
RUN chmod +x /opt/ilumina/*.sh

USER ilumina
WORKDIR /home/ilumina
VOLUME ["/state"]
EXPOSE 14500

HEALTHCHECK --interval=30s --timeout=5s --start-period=10m \
  CMD pgrep -f 'iLumina.exe' >/dev/null || exit 1

ENTRYPOINT ["/usr/bin/tini", "--", "/opt/ilumina/entrypoint.sh"]
CMD ["run"]
