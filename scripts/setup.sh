#!/usr/bin/env bash
set -e
echo "Running setup.sh"

# Scripts always recieve the same arguments from benchkit in the same order:

# BINARY="$1"
# CONNECT_ADDRESS="$2"
# NETWORK="$3"
# OUT_DIR="$4"
# SNAPSHOT_PATH="$5"
TMP_DATADIR="$6"
# ITERATION="$7"
# COMMIT="$8"

echo "Creating datadir ${TMP_DATADIR} with mkdir -p"
mkdir -p "${TMP_DATADIR}"

echo "Cleaning datadir contents from ${TMP_DATADIR}"
rm -Rf "${TMP_DATADIR:?}"/*
