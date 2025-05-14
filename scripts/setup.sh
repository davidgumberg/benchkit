#!/usr/bin/env bash
set -e
echo "Running setup.sh"

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

echo "Creating datadir ${TMP_DATADIR} with mkdir -p"
mkdir -p "${TMP_DATADIR}"

echo "Cleaning datadir contents from ${TMP_DATADIR}"
rm -Rf "${TMP_DATADIR:?}"/*
