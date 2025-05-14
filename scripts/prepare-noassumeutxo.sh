#!/usr/bin/env bash
set -ex
echo "Running prepare.sh"

# Process only needed parameters
while [ $# -gt 0 ]; do
  case "$1" in
    --datadir=*)
      TMP_DATADIR="${1#*=}"
      ;;
    # Accept but ignore other parameters
    --binary=* | --connect=* | --network=* | --snapshot=* | --out-dir=* | --iteration=* | --commit=* | --params-dir=*)
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
