#!/bin/sh
# =============================================================================
# Container entrypoint
#
# The persistent volume always wins: files already in /app/config and
# /app/data are used as they are and NEVER overwritten. Templates from the
# image are only copied when a file is missing (first run, or a new inverter).
# =============================================================================
set -e

APP_DIR="${APP_DIR:-/app}"
CONFIG_DIR="${CONFIG_DIR:-${APP_DIR}/config}"
DATA_DIR="${DATA_DIR:-${APP_DIR}/data}"
DEFAULTS_DIR="${DEFAULTS_DIR:-${APP_DIR}/config.defaults}"

log() { echo "[entrypoint] $*"; }

# ── Warn if a directory is not on a mounted volume (data would be lost) ───────
check_mount() {
    if ! grep -q " $1 " /proc/self/mountinfo 2>/dev/null; then
        log "WARNING: $1 is not a mounted volume — its contents will be LOST"
        log "         when the container is removed. Mount it in docker-compose.yml."
    fi
}
check_mount "$CONFIG_DIR"
check_mount "$DATA_DIR"

mkdir -p "$CONFIG_DIR" "$DATA_DIR"

for dir in "$CONFIG_DIR" "$DATA_DIR"; do
    if ! touch "${dir}/.write_test" 2>/dev/null; then
        log "ERROR: ${dir} is not writable — check the volume permissions"
        exit 1
    fi
    rm -f "${dir}/.write_test"
done

# ── Copy a template only if the target file does not exist ───────────────────
copy_if_missing() {
    if [ -f "$2" ]; then
        log "Using existing $(basename "$2") from the volume"
    else
        cp "$1" "$2"
        log "$(basename "$2") not found — created from the default template"
    fi
}

copy_if_missing "${DEFAULTS_DIR}/app_config.conf" "${CONFIG_DIR}/app_config.conf"

# One invNN_config.conf per inverter defined in app_config.conf ([[inverters]] id = N)
INV_IDS=$(grep -E '^[[:space:]]*id[[:space:]]*=[[:space:]]*[0-9]+' "${CONFIG_DIR}/app_config.conf" \
    | sed -E 's/[^0-9]*([0-9]+).*/\1/')
for id in ${INV_IDS:-1}; do
    copy_if_missing "${DEFAULTS_DIR}/inv_config.conf" \
        "${CONFIG_DIR}/$(printf 'inv%02d_config.conf' "$id")"
done

# Current templates as reference, refreshed on every start, so options added
# by a newer image can be seen without touching the real config files.
cp "${DEFAULTS_DIR}/app_config.conf" "${CONFIG_DIR}/app_config.conf.example"
cp "${DEFAULTS_DIR}/inv_config.conf" "${CONFIG_DIR}/inv_config.conf.example"

if [ -f "${DATA_DIR}/axpert.db" ]; then
    log "Using existing database from the volume"
else
    log "No database found — a new one will be created"
fi

log "Starting Axpert Multi-Inverter Monitor..."
cd "$APP_DIR"
exec ./axpert-monitor
