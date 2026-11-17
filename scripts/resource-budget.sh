#!/bin/sh
# Starts familiard on a private session bus, lets it settle, then checks idle
# memory and wakeups against the published budget. Usage:
#   scripts/resource-budget.sh [path/to/familiard]
set -eu

MAX_RSS_KIB=${MAX_RSS_KIB:-20480}
MAX_WAKEUPS=${MAX_WAKEUPS:-5}
WINDOW=${WINDOW:-10}
# Long enough for startup work and for idle tokio blocking threads to exit.
SETTLE=${SETTLE:-13}

bin=${1:-target/release/familiard}
[ -x "$bin" ] || { echo "no daemon binary at $bin; build with: cargo build --release -p familiard" >&2; exit 2; }

measure() {
    "$bin" --config "$rt/config.toml" >"$rt/daemon.log" 2>&1 &
    pid=$!
    sleep "$SETTLE"
    before=$(switches "$pid")
    sleep "$WINDOW"
    after=$(switches "$pid")
    rss=$(awk '/VmRSS/ { print $2 }' "/proc/$pid/status")
    threads=$(ls "/proc/$pid/task" | wc -l)
    kill "$pid"
    wait "$pid" 2>/dev/null || true
    wakeups=$((after - before))
    echo "idle memory ${rss} KiB (budget ${MAX_RSS_KIB}), wakeups ${wakeups} in ${WINDOW} s (budget ${MAX_WAKEUPS}), ${threads} threads"
    if [ "$rss" -gt "$MAX_RSS_KIB" ] || [ "$wakeups" -gt "$MAX_WAKEUPS" ]; then
        echo "over budget" >&2
        cat "$rt/daemon.log" >&2
        return 1
    fi
}

# Context switches summed over all threads; each idle wakeup adds at least one.
switches() {
    cat /proc/"$1"/task/*/status 2>/dev/null | awk '/ctxt_switches/ { s += $2 } END { print s }'
}

if [ -n "${FAMILIAR_BUDGET_RT:-}" ]; then
    rt=$FAMILIAR_BUDGET_RT
    measure
else
    # A private runtime dir keeps the private bus away from the real session.
    rt=$(mktemp -d)
    trap 'fusermount3 -u "$rt/doc" 2>/dev/null || true; rm -rf "$rt"' EXIT
    export MAX_RSS_KIB MAX_WAKEUPS WINDOW SETTLE
    XDG_RUNTIME_DIR="$rt" FAMILIAR_BUDGET_RT="$rt" dbus-run-session -- "$0" "$bin"
fi
