#!/bin/bash
# Calculate .DAD files with the real DACAD under Wine and save the results
# into the files (DACAD saves them as V2). Headless: runs on an Xvfb display.
#
#   DACAD_DIR=/path/to/DACAD tools/dacad_wine_calc.sh case1.DAD case2.DAD ...
#
# Needs: wine + wine32 (32-bit), Mesa GL for i386, Xvfb, xdotool, ImageMagick;
# DACAD's own SftTv32.DLL next to "DACAD DE.exe" and its licence key imported
# into the Wine registry (wine regedit <licence>.reg). A screenshot of the
# calculated plan is written next to each file as <name>.png.
#
# The UI is driven by fixed coordinates of the 1280x1024 layout: Ctrl+B runs
# the calculation (confirming the roof list if asked), then menu Data (17,36) -> Save (40,103).
set -euo pipefail
: "${DACAD_DIR:?set DACAD_DIR to the folder with DACAD DE.exe}"
export DISPLAY="${DISPLAY:-:7}" WINEDEBUG=-all
pgrep -x Xvfb >/dev/null || { Xvfb "$DISPLAY" -screen 0 1280x1024x24 +extension GLX >/dev/null 2>&1 & sleep 2; }

for f in "$@"; do
  f="$(realpath "$f")"
  wineserver -k 2>/dev/null || true; sleep 1
  (cd "$DACAD_DIR" && wine "DACAD DE.exe" "Z:${f//\//\\}" >/dev/null 2>&1 &)
  for _ in $(seq 1 30); do
    sleep 1; xdotool search --onlyvisible --name Grundriss >/dev/null 2>&1 && break
  done
  sleep 2
  before=$(stat -c %Y "$f")
  xdotool key ctrl+b; sleep 2
  # With chimneys or dormers DACAD first asks which roofs to recalculate
  # ("Neu berechnen", Slovak build "Prerátat"); OK keeps all of them.
  if xdotool search --onlyvisible --name 'Prer|berechnen' >/dev/null 2>&1; then
    xdotool key Return
  fi
  sleep 4
  xdotool key Escape
  xdotool mousemove 17 36 click 1; sleep 1; xdotool mousemove 40 103 click 1
  for _ in $(seq 1 4); do sleep 1; [ "$(stat -c %Y "$f")" != "$before" ] && break; done
  # Files from old DACAD versions open "Save as" (name preset) and then ask
  # to overwrite: accept both.
  if [ "$(stat -c %Y "$f")" = "$before" ]; then
    xdotool key Return; sleep 2
    [ "$(stat -c %Y "$f")" = "$before" ] && { xdotool key alt+y; sleep 1; xdotool key Return; }
    for _ in $(seq 1 6); do sleep 1; [ "$(stat -c %Y "$f")" != "$before" ] && break; done
  fi
  if [ "$(stat -c %Y "$f")" = "$before" ]; then echo "NOT SAVED $f" >&2; else echo "ok $f"; fi
  import -window root "${f%.*}.png"
  wineserver -k 2>/dev/null || true
done
