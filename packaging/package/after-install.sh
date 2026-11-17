#!/bin/sh
# Runs as root after the package is installed or upgraded.

# Applies the udev rule now, so the mouse can be reached without replugging it.
if command -v udevadm >/dev/null 2>&1; then
    udevadm control --reload || true
    udevadm trigger --subsystem-match=hidraw || true
    udevadm trigger --subsystem-match=misc --sysname-match=uinput || true
fi

# Starts the daemon with each user's graphical session, from their next login.
if command -v systemctl >/dev/null 2>&1; then
    systemctl --global enable familiard.service || true
fi

echo "Familiar starts when you next log in. To start it now: systemctl --user start familiard"
exit 0
