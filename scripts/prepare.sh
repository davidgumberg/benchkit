#!/usr/bin/env bash
# Exit on any error
# set -e
#
# # The first argument is alway the datadir
# # The second argument is always the run artifact dir
#
# # Check if we have the required arguments
# if [ "$#" -ne 2 ]; then
#     echo "Error: Required arguments missing"
#     echo "Usage: $0 <datadir> <artifact-dir>"
#     exit 1
# fi
#
# # Sanity check the datadir and debug.log
# if [ ! -d "$1" ]; then
#     echo "Error: Data directory '$1' does not exist"
#     exit 1
# fi
#
# # Check if artifact directory exists
# if [ ! -d "$2" ]; then
#     echo "Error: Artifact directory '$2' does not exist"
#     exit 1
# fi
#
# # Clean datadir
# rm -Rf "${1:?}"
exit 0
