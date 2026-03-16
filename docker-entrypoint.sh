#!/bin/sh
set -eu

if [ "$(id -u)" -eq 0 ]; then
    mkdir -p /data

    if ! chown appuser:appuser /data; then
        echo "warning: could not chown /data to appuser; checking write access" >&2
    fi

    if ! gosu appuser test -w /data; then
        echo "error: /data is not writable by appuser (uid 10001)" >&2
        exit 1
    fi

    exec gosu appuser "$@"
fi

exec "$@"
