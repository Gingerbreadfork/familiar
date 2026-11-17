#!/bin/sh
# Installs familiard and familiar for the current user and starts the daemon.
# With --app, also builds and installs the Familiar window.
# The udev rule needs root and is installed separately (see README).
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
bin="$HOME/.local/bin"
units="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
share="${XDG_DATA_HOME:-$HOME/.local/share}"
app=no
[ "${1:-}" = "--app" ] && app=yes

if [ "$app" = yes ] && ! command -v pnpm >/dev/null; then
    echo "--app needs pnpm; install it first" >&2
    exit 1
fi

cargo build --release --manifest-path "$root/Cargo.toml" -p familiard -p familiar-cli
mkdir -p "$bin" "$units"
install -m 755 "$root/target/release/familiard" "$root/target/release/familiar" "$bin/"
for unit in familiard.service familiar-haptic@.service; do
    sed -e "s|/usr/bin/familiard|$bin/familiard|" -e "s|/usr/bin/familiar|$bin/familiar|" \
        "$root/packaging/systemd/$unit" > "$units/$unit"
done

systemctl --user daemon-reload
systemctl --user enable familiard.service
systemctl --user restart familiard.service
systemctl --user --no-pager status familiard.service | head -5
if ! systemctl --user is-active --quiet familiard.service; then
    echo "familiard is not running; see journalctl --user -u familiard" >&2
    exit 1
fi

if [ "$app" = yes ]; then
    pnpm --dir "$root/gui" install --frozen-lockfile
    pnpm --dir "$root/gui" tauri build --no-bundle
    install -m 755 "$root/gui/src-tauri/target/release/familiar-gui" "$bin/"
    id=io.github.gingerbreadfork.Familiar
    mkdir -p "$share/applications"
    # Launchers may not have ~/.local/bin on their PATH.
    sed "s|^Exec=familiar-gui|Exec=$bin/familiar-gui|" "$root/packaging/$id.desktop" > "$share/applications/$id.desktop"
    install -Dm 644 "$root/gui/src-tauri/icons/32x32.png" "$share/icons/hicolor/32x32/apps/$id.png"
    install -Dm 644 "$root/gui/src-tauri/icons/128x128.png" "$share/icons/hicolor/128x128/apps/$id.png"
    install -Dm 644 "$root/gui/src-tauri/icons/128x128@2x.png" "$share/icons/hicolor/256x256/apps/$id.png"
    update-desktop-database "$share/applications" 2>/dev/null || true
fi
