#!/usr/bin/env bash
set -ex
echo "Running conclude.sh"

# Process only needed parameters
while [ $# -gt 0 ]; do
  case "$1" in
    --network=*)
      NETWORK="${1#*=}"
      ;;
    --out-dir=*)
      OUT_DIR="${1#*=}"
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
    --params-dir=*)
      PARAMS_DIR="${1#*=}"
      ;;
    # Accept but ignore other parameters
    --binary=* | --connect=* | --snapshot=*)
      # Just skip these parameters silently
      ;;
    *)
      echo "Unknown parameter: $1"
      exit 1
      ;;
  esac
  shift
done

# Move datadir files to the outdir in a structured way
# Use the directory structure: commit -> params -> iteration
echo "Moving debug.log to $OUT_DIR/$COMMIT/$PARAMS_DIR/$ITERATION/"
mkdir -p "$OUT_DIR"/"$COMMIT"/"$PARAMS_DIR"/"$ITERATION"

# Store debug.log in commit/params/iteration directory
if [ "$NETWORK" = "main" ]; then
    mv "$TMP_DATADIR"/debug.log "$OUT_DIR"/"$COMMIT"/"$PARAMS_DIR"/"$ITERATION"/debug.log
else
    mv "$TMP_DATADIR/$NETWORK/debug.log" "$OUT_DIR"/"$COMMIT"/"$PARAMS_DIR"/"$ITERATION"/debug.log
fi

echo "Cleaning datadir contents from ${TMP_DATADIR}"
rm -Rf "${TMP_DATADIR:?}"/*
