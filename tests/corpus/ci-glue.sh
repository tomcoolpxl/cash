#!/usr/bin/env bash
#
# A CI-glue-shaped script — the other workload §1 names, alongside Terraform wrappers.
#
# Where terraform-wrapper.sh exercises shell *language*, this exercises the shell as a
# process coordinator: pipelines, xargs, find, grep, sort, uniq, wc, redirection,
# subshells, and exit statuses flowing between them. Those are the pieces D48 bundles and
# D8 resolves, so a failure here means the userland story is broken rather than the
# parser.

set -euo pipefail

WORK="$(mktemp -d)"
trap 'cd /; rm -rf "$WORK"' EXIT

# --- a small source tree to operate on ----------------------------------------

mkdir -p "$WORK/src/lib" "$WORK/src/bin" "$WORK/docs"

cat > "$WORK/src/lib/alpha.rs" <<'EOF'
// TODO: refactor this
fn alpha() {}
EOF

cat > "$WORK/src/lib/beta.rs" <<'EOF'
fn beta() {}
EOF

cat > "$WORK/src/bin/main.rs" <<'EOF'
// TODO: handle errors
fn main() {}
EOF

cat > "$WORK/docs/readme.md" <<'EOF'
# docs
TODO: write these
EOF

# --- find | wc : counting files by type ---------------------------------------

RS_COUNT="$(find "$WORK/src" -name '*.rs' | wc -l | tr -d ' ')"
if [ "$RS_COUNT" -ne 3 ]; then
    echo "[ci] expected 3 rust files, found $RS_COUNT" >&2
    exit 1
fi

# --- grep across a pipeline ---------------------------------------------------

# A bare pipe into xargs, written exactly as it would be on Linux. On Windows this is
# where a backslash path would be destroyed — xargs reads \ as an escape — so keeping the
# unadorned form here is deliberate: it is the regression test for D48's path rendering.
TODO_COUNT="$(find "$WORK" -name '*.rs' | xargs grep -l 'TODO' | wc -l | tr -d ' ')"
if [ "$TODO_COUNT" -ne 2 ]; then
    echo "[ci] expected 2 rust files with TODOs, found $TODO_COUNT" >&2
    exit 1
fi

# --- sort | uniq over generated data ------------------------------------------

printf 'beta\nalpha\nbeta\ngamma\nalpha\n' > "$WORK/names.txt"
UNIQUE="$(sort "$WORK/names.txt" | uniq | wc -l | tr -d ' ')"
if [ "$UNIQUE" -ne 3 ]; then
    echo "[ci] expected 3 unique names, found $UNIQUE" >&2
    exit 1
fi

FIRST="$(sort "$WORK/names.txt" | uniq | head -1)"
if [ "$FIRST" != "alpha" ]; then
    echo "[ci] expected alpha first, got [$FIRST]" >&2
    exit 1
fi

# --- cut and tr on structured text --------------------------------------------

printf 'id,name,env\n1,web,prod\n2,api,staging\n' > "$WORK/hosts.csv"
ENVS="$(tail -n +2 "$WORK/hosts.csv" | cut -d, -f3 | sort | tr '\n' ' ')"
case "$ENVS" in
    "prod staging "*) ;;
    *)
        echo "[ci] unexpected envs: [$ENVS]" >&2
        exit 1
        ;;
esac

# --- a failing command in a pipeline, with and without pipefail ---------------

set +o pipefail
if false | true; then
    :
else
    echo "[ci] pipeline without pipefail should have succeeded" >&2
    exit 1
fi
set -o pipefail

if false | true; then
    echo "[ci] pipeline with pipefail should have failed" >&2
    exit 1
fi

# --- xargs with a command that takes many arguments ---------------------------

find "$WORK/src" -name '*.rs' -print0 | xargs -0 wc -l > "$WORK/lines.txt"
if [ ! -s "$WORK/lines.txt" ]; then
    echo "[ci] xargs produced nothing" >&2
    exit 1
fi

# --- a subshell that must not leak its cd --------------------------------------

BEFORE="$(pwd)"
(cd "$WORK/docs" && pwd > "$WORK/inner-pwd.txt")
AFTER="$(pwd)"

if [ "$BEFORE" != "$AFTER" ]; then
    echo "[ci] a subshell's cd leaked: $BEFORE -> $AFTER" >&2
    exit 1
fi

INNER="$(cat "$WORK/inner-pwd.txt")"
case "$INNER" in
    *docs) ;;
    *)
        echo "[ci] subshell pwd wrong: [$INNER]" >&2
        exit 1
        ;;
esac

# --- redirection of both streams ----------------------------------------------

{
    echo "to stdout"
    echo "to stderr" >&2
} > "$WORK/out.txt" 2> "$WORK/err.txt"

if [ "$(cat "$WORK/out.txt")" != "to stdout" ]; then
    echo "[ci] stdout redirection wrong" >&2
    exit 1
fi
if [ "$(cat "$WORK/err.txt")" != "to stderr" ]; then
    echo "[ci] stderr redirection wrong" >&2
    exit 1
fi

echo "[ci] rust=$RS_COUNT todos=$TODO_COUNT unique=$UNIQUE envs=${ENVS% }"
echo "[ci] ok"
