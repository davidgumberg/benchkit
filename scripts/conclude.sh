#!/usr/bin/env bash
set -e
echo "Running conclude.sh"

# Process named arguments
while [ $# -gt 0 ]; do
  case "$1" in
    --binary=*)
      BINARY="${1#*=}"
      ;;
    --connect=*)
      CONNECT_ADDRESS="${1#*=}"
      ;;
    --network=*)
      NETWORK="${1#*=}"
      ;;
    --out-dir=*)
      OUT_DIR="${1#*=}"
      ;;
    --snapshot=*)
      SNAPSHOT_PATH="${1#*=}"
      ;;
    --datadir=*)
      TMP_DATADIR="${1#*=}"
      ;;
    --iteration=*)
      ITERATION="${1#*=}"
      ;;
    --commit=*)
      COMMIT="${1#*=}"
      ;;
    *)
      echo "Unknown parameter: $1"
      exit 1
      ;;
  esac
  shift
done

# Move datadir files to the outdir
echo "Moving debug.log to $OUT_DIR/$COMMIT"
mkdir -p "$OUT_DIR"/"$COMMIT"
# Now we can include ITERATION in this filepath
if [ "$NETWORK" = "mainnet" ]; then
    mv "$TMP_DATADIR"/debug.log "$OUT_DIR"/"$COMMIT"/debug-${ITERATION}.log
else
    mv "$TMP_DATADIR/$NETWORK/debug.log" "$OUT_DIR"/"$COMMIT"/debug-${ITERATION}.log
fi

echo "Cleaning datadir contents from ${TMP_DATADIR}"
rm -Rf "${TMP_DATADIR:?}"/*
