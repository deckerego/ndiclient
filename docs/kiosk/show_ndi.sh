#!/bin/bash

while true; do
    source /opt/kiosk/config
    DISPLAY=:0 /usr/bin/ndiclient --ipaddr "$WIRED_IPADDR" --wait-image "$SPLASH_IMAGE" --fullscreen "$DEVICE_NAME" "$SOURCE_NAME"
    echo "ndiclient exited with $?, restarting..."
    sleep 30
done
