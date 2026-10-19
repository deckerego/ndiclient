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
