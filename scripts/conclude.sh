#!/usr/bin/env bash
set -e
echo "Running conclude.sh"

# Scripts always recieve the same arguments from benchkit in the same order:

# BINARY="$1"
# CONNECT_ADDRESS="$2"
NETWORK="$3"
OUT_DIR="$4"
# SNAPSHOT_PATH="$5"
TMP_DATADIR="$6"
# ITERATION="$7"
COMMIT="$8"

# Move datadir files to the outdir
echo "Moving debug.log to $OUT_DIR/$COMMIT"
mkdir -p "$OUT_DIR"/"$COMMIT"
# TODO: include $ITERATION in this filepath
if [ "$NETWORK" = "mainnet" ]; then
    mv "$TMP_DATADIR"/debug.log "$OUT_DIR"/"$COMMIT"/
else
    mv "$TMP_DATADIR/$NETWORK/debug.log" "$OUT_DIR"/"$COMMIT"/
fi

echo "Cleaning datadir contents from ${TMP_DATADIR}"
rm -Rf "${TMP_DATADIR:?}"/*
