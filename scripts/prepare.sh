#!/usr/bin/env bash
# Exit on any error
set -e

# The first argument is the datadir
# The second argument is the bitcoind
# The third argument is 

# Check if we have the required arguments
if [ "$#" -ne 2 ]; then
    echo "Error: Required arguments missing"
    exit 1
fi

# Sanity check the datadir and debug.log
if [ ! -d "$1" ]; then
    echo "Error: Data directory '$1' does not exist"
    exit 1
fi

# Check if artifact directory exists
if [ ! -d "$2" ]; then
    echo "Error: Artifact directory '$2' does not exist"
    exit 1
fi


# TMP_DATADIR="$1"
# UTXO_PATH="$2"
# CONNECT_ADDRESS="$3"
# CHAIN="$4"
# DBCACHE="$5"
# commit="$6"
# BINARIES_DIR="$7"

# Clean datadir contents
rm -Rf "${1:?}"/*

# Use the pre-built binaries from BINARIES_DIR
# "${BINARIES_DIR}/${commit}/bitcoind" --help
# taskset -c 0-15 "${BINARIES_DIR}/${commit}/bitcoind" -datadir="${TMP_DATADIR}" -connect="${CONNECT_ADDRESS}" -daemon=0 -chain="${CHAIN}" -stopatheight=1 -printtoconsole=0
# taskset -c 0-15 "${BINARIES_DIR}/${commit}/bitcoind" -datadir="${TMP_DATADIR}" -connect="${CONNECT_ADDRESS}" -daemon=0 -chain="${CHAIN}" -dbcache="${DBCACHE}" -pausebackgroundsync=1 -loadutxosnapshot="${UTXO_PATH}" -printtoconsole=0 || true
# clean_logs "${TMP_DATADIR}"
