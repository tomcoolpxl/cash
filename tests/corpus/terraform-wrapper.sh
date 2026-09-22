#!/usr/bin/env bash
#
# A Terraform-wrapper-shaped script, of the kind §1 names as the target workload.
#
# Written the way such a script is actually written on Linux — no Windows
# accommodations, no cash-specific spellings — because D2 promises unmodified
# POSIX-shaped scripts run as-is. Every construct here is one that appears in real
# infrastructure wrappers.
#
# It exercises, deliberately: set -euo pipefail, an EXIT trap doing cleanup, mktemp,
# command substitution that would pick up a CRLF, $(pwd) composed into an argument,
# case, functions, local variables, arrays, parameter expansion with defaults,
# arithmetic, a pipeline with a filter, a here-document, process substitution, and an
# exit status checked against a conditional.

set -euo pipefail

WORKDIR="$(mktemp -d)"
CLEANED=0

cleanup() {
    # The EXIT trap must actually fire, or the temp directory leaks.
    CLEANED=1
    rm -rf "$WORKDIR"
}
trap cleanup EXIT

log() {
    printf '[%s] %s\n' "${LOG_PREFIX:-tf}" "$*"
}

# --- environment and defaults ------------------------------------------------

ENVIRONMENT="${DEPLOY_ENV:-staging}"
REGION="${AWS_REGION:-eu-west-1}"
PARALLELISM="${TF_PARALLELISM:-10}"

case "$ENVIRONMENT" in
    prod|production)
        APPROVE=""
        ;;
    staging|dev)
        APPROVE="-auto-approve"
        ;;
    *)
        log "unknown environment: $ENVIRONMENT"
        exit 2
        ;;
esac

# --- a version file, as a repo would have it (with CRLF, as git on Windows leaves it)

printf 'v1.4.2\r\n' > "$WORKDIR/VERSION"
VERSION="$(cat "$WORKDIR/VERSION")"

if [ "$VERSION" != "v1.4.2" ]; then
    log "version mismatch: got [$VERSION]"
    exit 1
fi

# --- a module list, filtered through a pipeline ------------------------------

cat > "$WORKDIR/modules.txt" <<'EOF'
networking
compute
# a comment that must be filtered out
storage

database
EOF

MODULE_COUNT=0
while IFS= read -r module; do
    case "$module" in
        ''|'#'*) continue ;;
    esac
    MODULE_COUNT=$((MODULE_COUNT + 1))
done < "$WORKDIR/modules.txt"

if [ "$MODULE_COUNT" -ne 4 ]; then
    log "expected 4 modules, counted $MODULE_COUNT"
    exit 1
fi

# --- paths composed into arguments -------------------------------------------

cd "$WORKDIR"
CHDIR_ARG="-chdir=$(pwd)/modules"
mkdir -p modules

case "$CHDIR_ARG" in
    *" "*) ;;   # a path with spaces is fine, just do not split it
esac

# The wrapper would hand this to terraform; here we only assert it is well formed.
if [ -z "${CHDIR_ARG#-chdir=}" ]; then
    log "empty chdir argument"
    exit 1
fi

# --- arrays and parameter expansion ------------------------------------------

TARGETS=(module.networking module.compute)
TARGET_ARGS=""
for t in "${TARGETS[@]}"; do
    TARGET_ARGS="$TARGET_ARGS -target=$t"
done

if [ "${#TARGETS[@]}" -ne 2 ]; then
    log "array length wrong: ${#TARGETS[@]}"
    exit 1
fi

# --- a plan file, and a status checked conditionally --------------------------

plan() {
    # Simulates `terraform plan -detailed-exitcode`: 0 = no changes, 2 = changes.
    local module="$1"
    case "$module" in
        storage) return 2 ;;
        *) return 0 ;;
    esac
}

CHANGED=0
for module in networking compute storage database; do
    # `set -e` must not kill the script on a non-zero return inside a conditional.
    if plan "$module"; then
        :
    else
        status=$?
        if [ "$status" -eq 2 ]; then
            CHANGED=$((CHANGED + 1))
        else
            log "plan failed for $module with $status"
            exit 1
        fi
    fi
done

if [ "$CHANGED" -ne 1 ]; then
    log "expected 1 changed module, got $CHANGED"
    exit 1
fi

# --- process substitution, as a diff of expected vs actual --------------------

if ! diff <(printf 'a\nb\n') <(printf 'a\nb\n') > /dev/null 2>&1; then
    log "identical inputs compared unequal"
    exit 1
fi

if diff <(printf 'a\n') <(printf 'b\n') > /dev/null 2>&1; then
    log "differing inputs compared equal"
    exit 1
fi

# --- output ------------------------------------------------------------------

log "environment=$ENVIRONMENT region=$REGION parallelism=$PARALLELISM"
log "version=$VERSION modules=$MODULE_COUNT changed=$CHANGED"
log "approve=${APPROVE:-<none>} targets=$TARGET_ARGS"
log "ok"
