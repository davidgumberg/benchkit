#!/usr/bin/env bash
set -e
echo "Running prepare.sh"

# Scripts always recieve the same arguments from benchkit in the same order:

BINARY="$1"
CONNECT_ADDRESS="$2"
NETWORK="$3"
# OUT_DIR="$4"
SNAPSHOT_PATH="$5"
TMP_DATADIR="$6"
# ITERATION="$7"
# COMMIT="$8"

mkdir -p "${TMP_DATADIR}"
rm -Rf "${TMP_DATADIR:?}/*"

echo "Syncing headers"
taskset -c 0-15 "${BINARY}" -datadir="${TMP_DATADIR}" -connect="${CONNECT_ADDRESS}" -daemon=0 -chain="${NETWORK}" -stopatheight=1 -printtoconsole=0
echo "Loading snapshot"
taskset -c 0-15 "${BINARY}" -datadir="${TMP_DATADIR}" -connect="${CONNECT_ADDRESS}" -daemon=0 -chain="${NETWORK}" -pausebackgroundsync=1 -loadutxosnapshot="${SNAPSHOT_PATH}" -printtoconsole=0 || true
