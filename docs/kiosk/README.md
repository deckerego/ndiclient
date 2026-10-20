# Example Kiosk Scripts

It's pretty common to use `ndiclient` in "kiosk" mode, where it is
the only running app. This directory has a few scripts that
helps run the app on startup and keep it running.


## show_obs.sh

This is an example of how you might connect to an OBS source on a remote
device, restarting in case of failure. Tweak as needed; this can
be saved in `/home/pi` or whatever directory you have for an
autologin user.


## ndiclient.desktop

On the Raspberry Pi, move `ndiclient.desktop` to `.config/autostart/`
and modify the contents to match your description and path
to the run script above. If you set your desktop to autostart,
this will run the script immediately on boot.


## ufw Firewall Rules

NDI opens a wiiiiiiide array of ports for both the device/source
and client sides. The whole list is available at the
[NDI Related Network Ports](https://docs.ndi.video/all/getting-started/white-paper/ndi-related-network-ports)
list in the NDI standards doc.

If you would like to set up the minimum number of ports
for `ndiclient` assuming a fairly standard setup:
```
sudo ufw allow ssh
sudo ufw allow 5960:6959/tcp
sudo ufw allow 5960:6959/udp
```

Note multi-TCP or unicast UDP receiving may require
other or additional port ranges.


## HDMI Consumer Electronics Control

The utility `cec-utils` can be pretty handy to control televisions &
screens that obey Consumer Electronics Control (CEC) commands over HDMI.
For example, to check and see what devices are connected and
their current status you can send the command `scan` over HDMI:
```
echo scan | cec-client -s -d 1
```

Turning on or off the screen is fairly straightforward
(but pipes a lot of info to stdout):
```
echo "on 0" | cec-client -s
echo "standby 0" | cec-client -s
```