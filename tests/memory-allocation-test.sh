#!/bin/bash

echo "Allocating ~1GB of memory using dd and /dev/zero..."

tmpfile=$(mktemp /dev/shm/memtest.XXXXXX)
dd if=/dev/zero of="$tmpfile" bs=1M count=1024 status=none

echo "1GB allocated at $tmpfile. Press Ctrl+C to release."
while true; do sleep 1; done
