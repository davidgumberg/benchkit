#!/usr/bin/env bash
set -e
echo "Running setup.sh"

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

echo "Creating datadir ${TMP_DATADIR} with mkdir -p"
mkdir -p "${TMP_DATADIR}"

echo "Cleaning datadir contents from ${TMP_DATADIR}"
rm -Rf "${TMP_DATADIR:?}"/*
