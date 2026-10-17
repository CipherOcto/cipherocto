#!/bin/sh
# Entrypoint for a single node container in the Layer 4 multi-node suite.
#
# `octo` is a one-shot dispatcher: it has no `serve` subcommand and no
# daemon mode, so there is no octo process for this entrypoint to exec
# into. What a node container actually is, and what this script sets
# up, is:
#
#   1. an `$OCTO_HOME` on a per-node volume, created and owned by the
#      container user, so the node's peer table is durable across a
#      container restart and destroyed by `compose down -v`;
#   2. a network namespace on the compose bridge, under a DNS alias
#      the suite can put in a `tcp://` endpoint and then actually
#      connect to.
#
# The script then idles, holding the container up so `docker compose
# exec` has somewhere to run `octo`.
#
# It deliberately performs no `octo` invocation of its own. Warm-up
# belongs to the suite, which needs to observe the exit code and
# stderr of any bootstrap step to assert on it.

set -eu

: "${OCTO_HOME:=/octo/home}"
export OCTO_HOME

# Create the home with owner-only permissions, matching what the
# substrate itself does for the mesh directory. `umask` first so the
# directory is never briefly world-readable between create and chmod.
umask 077
mkdir -p "$OCTO_HOME"

# Fail fast and loudly if the volume is not writable: a node whose home
# silently lives on a read-only layer would make every write-path
# assertion vacuous.
if [ ! -w "$OCTO_HOME" ]; then
    echo "octo-node: OCTO_HOME $OCTO_HOME is not writable" >&2
    exit 70
fi

echo "octo-node: ready (OCTO_HOME=$OCTO_HOME)"

# If a command was passed, run it as this node's process instead of
# idling. `docker run <image> <cmd>` hands the command to the ENTRYPOINT
# as arguments, so without this branch the entrypoint silently ignores
# it and idles — the container appears to hang rather than running
# what was asked. `docker compose exec` is unaffected, since exec
# bypasses the entrypoint entirely.
if [ "$#" -gt 0 ]; then
    exec "$@"
fi

# Idle. `tail` on /dev/null blocks indefinitely and reaps nothing, so
# the container stays up until the suite stops it. Signals from
# `docker stop` are delivered to this shell, which is what lets
# `compose stop` / `compose start` model a partition and heal.
exec tail -f /dev/null
