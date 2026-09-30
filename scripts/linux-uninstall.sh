#!/bin/sh
set -eu
bin="${HOME}/.local/bin"
data="${XDG_DATA_HOME:-${HOME}/.local/share}"
rm -f -- "$bin/bingee-desktop" "$data/applications/bingee-desktop.desktop" "$data/icons/hicolor/scalable/apps/bingee-desktop.svg"
echo "Removed application files. Library, cache and logs remain in user directories."
