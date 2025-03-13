#!/usr/bin/env bash
set -e
echo "Running cleanup.sh"

# Scripts always recieve the same arguments from benchkit in the same order:

# BINARY="$1"
# CONNECT_ADDRESS="$2"
# NETWORK="$3"
# OUT_DIR="$4"
# SNAPSHOT_PATH="$5"
TMP_DATADIR="$6"
# ITERATION="$7"
# COMMIT="$8"

rm -Rf "${TMP_DATADIR:?}"/*
