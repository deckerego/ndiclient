#!/bin/sh
# Builds the development test-pattern NDI sender into target/test_pattern_sender.
set -e
SDK_DIR="${NDI_SDK_DIR:-/home/pi/NDI SDK for Linux}"
LIBRARY_DIR="$SDK_DIR/lib/aarch64-rpi4-linux-gnueabi"
PROJECT_DIR="$(dirname "$0")/.."

mkdir -p "$PROJECT_DIR/target"
gcc -std=c11 -O2 -Wall -Wextra -I"$SDK_DIR/include" \
    "$PROJECT_DIR/tools/test_pattern_sender.c" \
    -L"$LIBRARY_DIR" -Wl,-rpath,"$LIBRARY_DIR" -lndi -lm \
    -o "$PROJECT_DIR/target/test_pattern_sender"
