#!/usr/bin/env bash
set -e

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

# TODO: I think this should be binary, not bin-dir...
BINARY="$1"
CONNECT_ADDRESS="$2"
NETWORK="$3"
SNAPSHOT_PATH="$4"
TMP_DATADIR="$5"

# Use the pre-built binaries from BINARIES_DIR
"${BINARY}" --help
# TODO: Cheange print-to-console back
taskset -c 0-15 "${BINARY}" -datadir="${TMP_DATADIR}" -connect="${CONNECT_ADDRESS}" -daemon=0 -chain="${NETWORK}" -stopatheight=1 -printtoconsole=1
taskset -c 0-15 "${BINARY}" -datadir="${TMP_DATADIR}" -connect="${CONNECT_ADDRESS}" -daemon=0 -chain="${NETWORK}" -pausebackgroundsync=1 -loadutxosnapshot="${SNAPSHOT_PATH}" -printtoconsole=1 || true

exit 0
