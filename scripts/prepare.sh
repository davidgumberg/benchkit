#!/usr/bin/env bash
set -e
echo "Running prepare.sh"

# Process only needed parameters
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
    --snapshot=*)
      SNAPSHOT_PATH="${1#*=}"
      ;;
    --datadir=*)
      TMP_DATADIR="${1#*=}"
      ;;
    # Accept but ignore other parameters
    --out-dir=* | --iteration=* | --commit=* | --params-dir=*)
      # Just skip these parameters silently
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
"${BINARY}" -datadir="${TMP_DATADIR}" -connect="${CONNECT_ADDRESS}" -daemon=0 -chain="${NETWORK}" -stopatheight=1 -printtoconsole=0
echo "Loading snapshot"
"${BINARY}" -datadir="${TMP_DATADIR}" -connect="${CONNECT_ADDRESS}" -daemon=0 -chain="${NETWORK}" -pausebackgroundsync=1 -loadutxosnapshot="${SNAPSHOT_PATH}" -printtoconsole=0 || true
