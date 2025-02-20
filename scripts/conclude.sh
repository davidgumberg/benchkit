#!/usr/bin/env bash
# Exit on any error
set -e

# The first argument is alway the datadir
# The second argument is always the run artifact dir

# Check if we have the required arguments
if [ "$#" -ne 2 ]; then
    echo "Error: Required arguments missing"
    echo "Usage: $0 <datadir> <artifact-dir>"
    exit 1
fi

# Sanity check the datadir and debug.log
if [ ! -d "$1" ]; then
    echo "Error: Data directory '$1' does not exist"
    exit 1
fi

if [ ! -f "$1/debug.log" ]; then
    echo "Error: debug.log not found in '$1'"
    echo "Data directory appears to be invalid or incomplete"
    exit 1
fi

# Check if artifact directory exists
if [ ! -d "$2" ]; then
    echo "Error: Artifact directory '$2' does not exist"
    exit 1
fi

# TODO: run parse_and_plot.py

# Next we move datadir files
mv "$1"/debug.log "$2"/

# Clean datadir
rm -Rf "${1:?}"
