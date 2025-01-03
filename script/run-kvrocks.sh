#!/bin/bash

VOLUME="kvrocks"
PORT=6666
WORKERS=$(fgrep 'processor' /proc/cpuinfo | wc -l)

docker volume create "$VOLUME"
docker run --name kvrocks --volume "$VOLUME:/var/lib/kvrocks" -it -d -p 6666:$PORT --restart unless-stopped apache/kvrocks:latest --bind 0.0.0.0 --workers $WORKERS

