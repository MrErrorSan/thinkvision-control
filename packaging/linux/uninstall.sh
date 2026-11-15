#!/bin/sh
# Remove ThinkVision Control for the current user: everything install.sh installed, plus the start-at-login entry.
# Settings and logs in ~/.config/ThinkVisionControl are kept; delete that folder as well for a clean slate.
# Quit the app first (tray menu → Exit): a running copy keeps going until then.
set -eu
config="${XDG_CONFIG_HOME:-$HOME/.config}"

rm -rf "$HOME/.local/share/thinkvision-control"
rm -f "$HOME/.local/bin/thinkvision-control" \
      "$HOME/.local/share/applications/thinkvision-control.desktop" \
      "$HOME/.local/share/icons/hicolor/scalable/apps/thinkvision-control.svg" \
      "$config/autostart/thinkvision-control.desktop"
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$HOME/.local/share/applications" || true
fi
echo "Removed ThinkVision Control. Your settings are still in $config/ThinkVisionControl."

rules=/etc/udev/rules.d/60-thinkvision-i2c.rules
[ -e "$rules" ] || exit 0
echo
echo "The monitor access rule ($rules) is still installed. Removing it needs sudo."
echo "(/etc/modules-load.d/i2c-dev.conf stays: ddcutil and other tools use the i2c-dev module too.)"
answer=n   # without a terminal, print the commands instead of running sudo
if [ -t 0 ]; then
    printf "Remove it now? [y/N] "
    read -r answer || answer=n
fi
case "$answer" in
    y|Y|yes|Yes)
        sudo rm -f "$rules"
        sudo udevadm control --reload
        echo "Done."
        ;;
    *)
        echo "Later, run:"
        echo "  sudo rm $rules && sudo udevadm control --reload"
        ;;
esac
