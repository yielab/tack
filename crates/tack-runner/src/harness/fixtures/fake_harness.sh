#!/bin/sh
# Shared fake harness binary for D1 (Codex), D2 (Claude Code) and D3
# (OpenCode) deterministic CI tests, and for D4's own process/event-sink
# tests. POSIX `sh`, not bash: no compilation step, always runnable, and one
# behavior for every OS this runner targets (see the D4 handoff and
# `crates/tack-runner/src/harness/fixtures/mod.rs` for the Rust-side
# accessors `fake_harness_path()`/`fake_harness_command()`).
#
# Every knob is an environment variable, never an argv flag: this leaves
# argv free for each adapter's real invocation shape (e.g. mimicking
# `codex exec --json ...`) so an adapter's own tests can assert on its real
# argv construction while still driving this fixture underneath it.
#
# Modes (TACK_FAKE_HARNESS_MODE, default "success"):
#
#   success        Prints "fake-harness-ok" to stdout, exits 0.
#   failure        Prints an error to stderr, exits
#                  TACK_FAKE_HARNESS_EXIT_CODE (default 1).
#   version        Prints TACK_FAKE_HARNESS_VERSION (default "1.0.0") to
#                  stdout, exits 0. For ordinary version-detection tests.
#   unknown_version
#                  Prints an unrecognized/future version string to stdout,
#                  exits 0. For an adapter's "unknown installed version"
#                  fake test.
#   malformed      Prints deliberately unparseable/mixed garbage to stdout
#                  (not valid JSON, not a recognizable version string),
#                  exits 0. For an adapter's malformed-output fake test.
#   hang           Sleeps TACK_FAKE_HARNESS_SLEEP_SECONDS (default 3600,
#                  i.e. "effectively forever" — the caller must cancel or
#                  time it out, never wait for natural exit). Single
#                  process image (execs into `sleep`).
#   spawn_child    Spawns a background `sleep TACK_FAKE_HARNESS_SLEEP_SECONDS`
#                  *without* detaching it into a new process group (so it
#                  inherits this script's group), writes that child's pid to
#                  TACK_FAKE_HARNESS_PIDFILE, then waits on it. This is the
#                  "child that spawns its own child" fixture for
#                  process-group cancellation tests: the caller's spawned
#                  process is this script; the script's own child is the
#                  grandchild relative to the caller.
#   high_volume    Writes exactly TACK_FAKE_HARNESS_VOLUME_BYTES (default
#                  50000000) bytes of repeated non-secret 'x' characters to
#                  stdout as fast as possible, then exits 0. For
#                  memory-bound capture tests.
#   echo_canary    Echoes back, to *both* stdout and stderr: the value of
#                  every environment variable named in
#                  TACK_FAKE_HARNESS_ECHO_ENV_KEYS (comma- or
#                  space-separated), and the full contents of stdin if any
#                  was provided. Simulates a worst-case leaky harness that
#                  prints its own credentials/prompt for "debugging", so a
#                  caller can prove its redaction layer scrubs it anyway.
#                  Exits 0.
#   read_relative  Reads the file at TACK_FAKE_HARNESS_READ_PATH, resolved
#                  relative to the current working directory, and prints its
#                  contents to stdout. Exits 0 if found, 1 (or
#                  TACK_FAKE_HARNESS_EXIT_CODE) with a stderr message if not.
#                  For workspace-confinement tests: the caller controls cwd
#                  via ProcessSpec::working_directory, this mode never does
#                  its own path confinement.
#
# Every mode first prints one diagnostic line to stderr
# (`fake_harness: mode=<mode> pid=$$`) — never secret, always safe to leave
# in captured output; useful when a test fails and needs to know which mode
# actually ran.

mode="${TACK_FAKE_HARNESS_MODE:-success}"
echo "fake_harness: mode=$mode pid=$$" >&2

case "$mode" in
  success)
    echo "fake-harness-ok"
    exit 0
    ;;

  failure)
    echo "fake-harness-failure" >&2
    exit "${TACK_FAKE_HARNESS_EXIT_CODE:-1}"
    ;;

  version)
    echo "${TACK_FAKE_HARNESS_VERSION:-1.0.0}"
    exit 0
    ;;

  unknown_version)
    echo "harness-cli version 999.999.999-nightly-exotic-format"
    exit 0
    ;;

  malformed)
    printf '%s' '{"incomplete": true, "trailing_garbage": '
    printf '\x00\x01 not-json-at-all }}}\n'
    echo "another line that is not JSON either"
    exit 0
    ;;

  hang)
    exec sleep "${TACK_FAKE_HARNESS_SLEEP_SECONDS:-3600}"
    ;;

  spawn_child)
    sleep "${TACK_FAKE_HARNESS_SLEEP_SECONDS:-3600}" &
    child_pid=$!
    if [ -n "$TACK_FAKE_HARNESS_PIDFILE" ]; then
      echo "$child_pid" > "$TACK_FAKE_HARNESS_PIDFILE"
    fi
    wait "$child_pid"
    ;;

  high_volume)
    bytes="${TACK_FAKE_HARNESS_VOLUME_BYTES:-50000000}"
    head -c "$bytes" /dev/zero | tr '\0' 'x'
    exit 0
    ;;

  echo_canary)
    for key in $(printf '%s' "${TACK_FAKE_HARNESS_ECHO_ENV_KEYS:-}" | tr ',' ' '); do
      value=$(eval echo "\$$key")
      echo "env:$key=$value"
      echo "env:$key=$value" >&2
    done
    stdin_content=$(cat)
    if [ -n "$stdin_content" ]; then
      echo "stdin=$stdin_content"
      echo "stdin=$stdin_content" >&2
    fi
    exit 0
    ;;

  read_relative)
    path="$TACK_FAKE_HARNESS_READ_PATH"
    if [ -f "$path" ]; then
      cat "$path"
      exit 0
    else
      echo "fake_harness: read_relative could not find $path" >&2
      exit "${TACK_FAKE_HARNESS_EXIT_CODE:-1}"
    fi
    ;;

  *)
    echo "fake_harness: unknown TACK_FAKE_HARNESS_MODE '$mode'" >&2
    exit 64
    ;;
esac
