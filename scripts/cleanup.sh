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

# Clean tmp_data_dir contents
rm -Rf "${5:?}"/*
exit 0
