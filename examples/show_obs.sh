#!/bin/sh

WIRED_IPADDR='172.16.0.2'
DEVICE_NAME='DEVICE.LOCAL'
SOURCE_NAME='OBS PGM'
SPLASH_IMAGE="$HOME/logo.bmp"

export DISPLAY=:0

while true; do
    ndiclient --ipaddr "$WIRED_IPADDR" --wait-image "$SPLASH_IMAGE" --fullscreen "$DEVICE_NAME" "$SOURCE_NAME"
    echo "ndiclient exited with $?, restarting..."
    sleep 5
done
