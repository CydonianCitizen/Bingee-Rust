#!/bin/sh
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
bin="${HOME}/.local/bin"
data="${XDG_DATA_HOME:-${HOME}/.local/share}"
install -d "$bin" "$data/applications" "$data/icons/hicolor/scalable/apps" "$data/doc/bingee-desktop"
install -m 755 "$root/bin/bingee-desktop" "$bin/bingee-desktop"
install -m 644 "$root/share/icons/hicolor/scalable/apps/bingee-desktop.svg" "$data/icons/hicolor/scalable/apps/bingee-desktop.svg"
cp -R "$root/share/doc/bingee-desktop/." "$data/doc/bingee-desktop/"
cat > "$data/applications/bingee-desktop.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Bingee Desktop
Exec="$bin/bingee-desktop"
Icon=bingee-desktop
Terminal=false
Categories=AudioVideo;Video;
EOF
echo "Installed Bingee Desktop. Profile data remains in user data directories."
