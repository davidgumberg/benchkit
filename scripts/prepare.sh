#!/usr/bin/env bash
set -e
echo "Running prepare.sh"

# Scripts always recieve the same arguments from benchkit in the same order:
#
# pub struct ScriptArgs {
#     pub binary: String,
#     pub connect_address: String,
#     pub network: String,
#     pub snapshot_path: PathBuf,
#     pub tmp_data_dir: PathBuf,
# }

if [ "$#" -ne 5 ]; then
    echo "Error: Required arguments missing"
    exit 1
fi

BINARY="$1"
CONNECT_ADDRESS="$2"
NETWORK="$3"
SNAPSHOT_PATH="$4"
TMP_DATADIR="$5"
# echo "BINARY: ${BINARY}"
# echo "CONNECT_ADDRESS: ${CONNECT_ADDRESS}"
# echo "NETWORK: ${NETWORK}"
# echo "SNAPSHOT_PATH: ${SNAPSHOT_PATH}"
# echo "TMP_DATADIR: ${TMP_DATADIR}"

mkdir -p "${TMP_DATADIR}"
rm -Rf "${TMP_DATADIR:?}/*"

echo "Syncing headers"
taskset -c 0-15 "${BINARY}" -datadir="${TMP_DATADIR}" -connect="${CONNECT_ADDRESS}" -daemon=0 -chain="${NETWORK}" -stopatheight=1 -printtoconsole=0
echo "Loading snapshot"
taskset -c 0-15 "${BINARY}" -datadir="${TMP_DATADIR}" -connect="${CONNECT_ADDRESS}" -daemon=0 -chain="${NETWORK}" -pausebackgroundsync=1 -loadutxosnapshot="${SNAPSHOT_PATH}" -printtoconsole=0 || true
