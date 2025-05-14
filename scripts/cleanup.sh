#!/usr/bin/env bash
set -e
echo "Running cleanup.sh"

# Process only needed parameters
while [ $# -gt 0 ]; do
  case "$1" in
    --datadir=*)
      TMP_DATADIR="${1#*=}"
      ;;
    # Accept but ignore other parameters
    --binary=* | --connect=* | --network=* | --out-dir=* | --snapshot=* | --iteration=* | --commit=* | --params-dir=*)
      # Just skip these parameters silently
      ;;
    *)
      echo "Unknown parameter: $1"
      exit 1
      ;;
  esac
  shift
done

echo "Final cleanup of datadir ${TMP_DATADIR}"
rm -Rf "${TMP_DATADIR:?}"/*
