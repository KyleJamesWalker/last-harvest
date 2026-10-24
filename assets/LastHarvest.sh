#!/bin/bash
# Stock menu launcher: the frontend exits before this runs and restarts after.
cd "$(dirname "$0")/last-harvest" || exit 1
. /etc/profile.d/myenv.sh
./last-harvest > log.txt 2>&1
