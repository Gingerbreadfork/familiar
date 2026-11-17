#!/bin/sh
# Runs as root before the package is removed or upgraded. Debian passes
# "remove" or "upgrade"; rpm passes the number of copies that will remain.
case "${1:-}" in
    remove | purge | 0)
        if command -v systemctl >/dev/null 2>&1; then
            systemctl --global disable familiard.service || true
        fi
        ;;
esac
exit 0
