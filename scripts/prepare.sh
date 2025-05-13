#!/usr/bin/env bash
set -e
echo "Running prepare.sh"

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

mkdir -p "${TMP_DATADIR}"
rm -Rf "${TMP_DATADIR:?}/*"

echo "Syncing headers"
taskset -c 0-15 "${BINARY}" -datadir="${TMP_DATADIR}" -connect="${CONNECT_ADDRESS}" -daemon=0 -chain="${NETWORK}" -stopatheight=1 -printtoconsole=0
echo "Loading snapshot"
taskset -c 0-15 "${BINARY}" -datadir="${TMP_DATADIR}" -connect="${CONNECT_ADDRESS}" -daemon=0 -chain="${NETWORK}" -pausebackgroundsync=1 -loadutxosnapshot="${SNAPSHOT_PATH}" -printtoconsole=0 || true
