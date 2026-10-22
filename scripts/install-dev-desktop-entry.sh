#!/usr/bin/env sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
data_home=${XDG_DATA_HOME:-"$HOME/.local/share"}
binary=${1:-"$repo_root/target/debug/rspotify_client"}
if [ "$#" -gt 1 ]; then
    printf 'Usage: %s [path-to-rspotify_client]\n' "$0" >&2
    exit 1
fi
if [ ! -x "$binary" ]; then
    printf 'Build first with cargo build, or supply an executable path: %s\n' "$binary" >&2
    exit 1
fi
applications_dir="$data_home/applications"
icon_dir="$data_home/icons/hicolor/scalable/apps"

mkdir -p "$applications_dir" "$icon_dir"
install -m 0644 "$repo_root/assets/icons/rspotify.svg" "$icon_dir/rspotify.svg"

# Use installed absolute paths: a cargo-built executable is usually not on PATH, and an explicit
# icon path avoids a long-running panel's cached missing theme-icon lookup.
python3 - "$repo_root/data/rspotify.desktop" "$applications_dir/rspotify.desktop" "$binary" "$icon_dir/rspotify.svg" <<'PY'
import os
import pathlib
import sys

template, destination, binary, icon = map(pathlib.Path, sys.argv[1:])

def desktop_string(value):
    return value.replace("\\", "\\\\").replace("\n", "\\n").replace("\r", "\\r").replace("\t", "\\t")

executable = str(binary.resolve()).replace("%", "%%")
quoted = '"' + ''.join("\\" + c if c in '\\"`$' else c for c in executable) + '"'
values = {"Exec": desktop_string(quoted), "Icon": desktop_string(str(icon.resolve()))}
lines = []
for line in template.read_text().splitlines():
    key = line.partition("=")[0]
    lines.append(f"{key}={values[key]}" if key in values else line)
destination.write_text("\n".join(lines) + "\n")
os.chmod(destination, 0o644)
PY

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "$data_home/icons/hicolor" >/dev/null 2>&1 || true
fi

# KDE caches desktop entries separately; refresh its service database so the task manager can
# associate the Wayland app_id with this desktop file.
if command -v kbuildsycoca6 >/dev/null 2>&1; then
    kbuildsycoca6 --noincremental
elif command -v kbuildsycoca5 >/dev/null 2>&1; then
    kbuildsycoca5 --noincremental
fi

# kbuildsycoca only refreshes service metadata, not the running Plasma panel's icon loader.
desktop=${XDG_CURRENT_DESKTOP:-}
if [ "${desktop#*KDE}" != "$desktop" ] && command -v dbus-send >/dev/null 2>&1; then
    dbus-send --session --type=signal /KIconLoader org.kde.KIconLoader.iconChanged int32:0
fi

printf 'Installed rspotify desktop entry and bundled icon under %s\n' "$data_home"
