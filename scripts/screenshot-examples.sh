#!/bin/bash
set -euo pipefail
echo "Starting weston headless..."
weston --xwayland --width=1280 --height=800 --backend=headless --socket=wayland-1 --config=/dev/null 2>/dev/null &
sleep 1
# Find new display
NEW_DISP=$(ls -t /tmp/.X11-unix/ | head -1 | sed 's/X//' | tr -d 'X')
if [ -z "$NEW_DISP" ]; then
  echo "Using DISPLAY=:0"
  NEW_DISP=":0"
else
  NEW_DISP=":$NEW_DISP"
fi
export DISPLAY=$NEW_DISP
echo "Using DISPLAY=$DISPLAY"

for name in veloxc-init-default dashboard markdown-editor task-board settings-panel; do
  echo "--- Capturing $name ---"
  cargo run --release -p "velox-example-$name" -- $DISPLAY 2>/dev/null &
  PID=$!
  sleep 2.5
  import -window root -crop 1280x800+0+0 /tmp/${name}-window.png 2>/dev/null || true
  kill $PID 2>/dev/null || true
  sleep 0.5
done
echo "Done. Check /tmp/*.png"
