#!/usr/bin/env bash
#
# dev-worktree.sh — prepare and run an OpenPosterDB git worktree for local dev.
#
# A freshly created git worktree is missing all the gitignored bits that the
# app needs (api/.env, api/db, api/cache, web/node_modules). This script makes
# a new worktree runnable in one command:
#
#   1. Copies api/.env from the MAIN worktree if this worktree doesn't have one.
#   2. Installs web dependencies (npm) if web/node_modules is missing.
#   3. Runs the Rust API (api/) and the Vite web dev server (web/) together.
#      Ctrl+C — or either process exiting on its own — stops both.
#
# Run it from inside the worktree you want to set up (any subdirectory is fine).

set -euo pipefail

usage() {
    cat <<'EOF'
dev-worktree.sh — prepare and run an OpenPosterDB worktree for local dev.

Usage:
  ./scripts/dev-worktree.sh              Set up (if needed) then run API + web.
  ./scripts/dev-worktree.sh --fresh      Also delete this worktree's api/db and
                                          api/cache first, for a clean slate.
  ./scripts/dev-worktree.sh --release     Build/run the API in release mode.
  ./scripts/dev-worktree.sh --setup-only  Copy env + install deps, but don't run.
  ./scripts/dev-worktree.sh --yes         Skip the --fresh confirmation prompt
                                          (needed for non-interactive use).
  ./scripts/dev-worktree.sh --help        Show this help.

Note: --fresh in the MAIN worktree deletes your primary db/cache, so it asks
for confirmation first (or pass --yes). In a linked worktree it wipes without
asking.

What it does:
  1. Copies api/.env from the main worktree if this one lacks it.
  2. Installs web deps (npm) if web/node_modules is missing.
  3. Runs the Rust API (api/) and the Vite dev server (web/) together;
     Ctrl+C — or either process exiting — stops both.

Run it from inside the worktree you want to set up.
EOF
}

# --- Parse arguments --------------------------------------------------------
FRESH=0
SETUP_ONLY=0
ASSUME_YES=0
CARGO_RELEASE=""   # "" = debug build, "--release" = release build

while [ $# -gt 0 ]; do
    case "$1" in
        --fresh|--clean)  FRESH=1 ;;
        --release)        CARGO_RELEASE="--release" ;;
        --setup-only)     SETUP_ONLY=1 ;;
        -y|--yes|--force) ASSUME_YES=1 ;;
        -h|--help)        usage; exit 0 ;;
        *)
            echo "Unknown option: $1" >&2
            echo "Run '$0 --help' for usage." >&2
            exit 2
            ;;
    esac
    shift
done

# --- Locate the current and main worktrees ----------------------------------
if ! WT="$(git rev-parse --show-toplevel 2>/dev/null)"; then
    echo "Error: not inside a git repository. cd into your worktree first." >&2
    exit 1
fi

# The main worktree is the parent directory of the shared git dir. For a linked
# worktree, --git-common-dir resolves to <main>/.git; for the main worktree it
# is the main repo's own .git. Either way, dirname gives the main worktree root.
# Capture into its own variable (rather than nesting inside dirname) so a git
# failure — e.g. git older than 2.31, which lacks --path-format — surfaces as a
# clear error instead of silently degrading MAIN_ROOT to ".".
if ! COMMON_GIT_DIR="$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null)" || [ -z "$COMMON_GIT_DIR" ]; then
    echo "Error: could not determine the main worktree (git rev-parse failed; needs git >= 2.31)." >&2
    exit 1
fi
MAIN_ROOT="$(dirname "$COMMON_GIT_DIR")"

if [ ! -d "$WT/api" ] || [ ! -d "$WT/web" ]; then
    echo "Error: $WT does not look like the OpenPosterDB repo (missing api/ or web/)." >&2
    exit 1
fi

echo ">> Worktree : $WT"
echo ">> Main repo: $MAIN_ROOT"

# --- Tooling check ----------------------------------------------------------
for tool in cargo npm; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "Error: '$tool' not found in PATH." >&2
        exit 1
    fi
done

# --- 1. Copy api/.env if this worktree doesn't have one ---------------------
SRC_ENV="$MAIN_ROOT/api/.env"
DST_ENV="$WT/api/.env"

if [ -f "$DST_ENV" ]; then
    echo ">> api/.env already present — leaving it untouched."
elif [ "$WT" = "$MAIN_ROOT" ]; then
    echo "Error: this is the main worktree and it has no api/.env." >&2
    echo "       Create it first:  (cd \"$WT/api\" && cp .env.example .env)  then fill in your keys." >&2
    exit 1
elif [ ! -d "$MAIN_ROOT/api" ]; then
    # MAIN_ROOT isn't a usable checkout (e.g. a bare-repo-centric layout where
    # the common git dir's parent is not a worktree). Point at THIS worktree.
    echo "Error: couldn't find a main worktree with api/ to copy .env from (looked in $MAIN_ROOT)." >&2
    echo "       Create api/.env directly in this worktree:" >&2
    echo "       (cd \"$WT/api\" && cp .env.example .env)  then fill in your keys." >&2
    exit 1
elif [ -f "$SRC_ENV" ]; then
    cp "$SRC_ENV" "$DST_ENV"
    echo ">> Copied api/.env from main worktree -> $DST_ENV"
else
    echo "Error: the main worktree has no api/.env to copy ($SRC_ENV)." >&2
    echo "       Create it first:  (cd \"$MAIN_ROOT/api\" && cp .env.example .env)  then fill in your keys." >&2
    exit 1
fi

# --- 2. Optional fresh start: wipe this worktree's local DB + cache ---------
if [ "$FRESH" -eq 1 ]; then
    # In the main worktree, --fresh would delete your PRIMARY db/cache, so
    # require confirmation there — an interactive prompt, or --yes for
    # non-interactive use. In a linked worktree the data is throwaway, so wipe
    # it without asking.
    if [ "$WT" = "$MAIN_ROOT" ] && [ "$ASSUME_YES" -ne 1 ]; then
        echo "Warning: --fresh on the main worktree deletes your PRIMARY local data:" >&2
        echo "           $WT/api/db" >&2
        echo "           $WT/api/cache" >&2
        if [ -t 0 ]; then
            printf "Delete them and start fresh? [y/N] " >&2
            read -r reply || reply=""
            case "$reply" in
                [yY]|[yY][eE][sS]) ;;
                *) echo "Aborted — your data was left untouched." >&2; exit 1 ;;
            esac
        else
            echo "Refusing: not an interactive terminal. Re-run with --yes to confirm." >&2
            exit 1
        fi
    fi
    echo ">> --fresh: wiping this worktree's local data before starting."
    for sub in db cache; do
        target="$WT/api/$sub"
        if [ -e "$target" ]; then
            echo "   removing $target"
            rm -rf "$target"
        else
            echo "   (nothing at $target)"
        fi
    done
fi

# --- 3. Install web dependencies if missing ---------------------------------
if [ ! -d "$WT/web/node_modules" ]; then
    echo ">> Installing web dependencies (first run for this worktree)..."
    if ! ( cd "$WT/web" && npm ci ); then
        echo ">> 'npm ci' failed; falling back to 'npm install'..."
        ( cd "$WT/web" && npm install )
    fi
fi

if [ "$SETUP_ONLY" -eq 1 ]; then
    echo ">> Setup complete (--setup-only). Run again without it to start the servers."
    exit 0
fi

# --- 4. Run the API and the web dev server together -------------------------
# Both servers run as background subshells. On shutdown we must tear down the
# whole tree (cargo + the API binary, npm + vite), not just the subshells.
#
# When this script leads its own process group — the normal case when it is run
# as a foreground command from an interactive shell — `kill 0` signals every
# process in that group, reaching all descendants cleanly. If instead it was
# launched without job control (from a wrapper script, a Makefile, an npm
# script, nohup, or any non-interactive parent) it shares the caller's process
# group, where `kill 0` would also signal unrelated processes. We detect that
# case and fall back to killing the two services (and their children) directly.
SCRIPT_PGID="$(ps -o pgid= -p "$$" 2>/dev/null | tr -d ' ')" || SCRIPT_PGID=""
API_PID=""
WEB_PID=""

# Terminate a process and all of its descendants, deepest first, so that a
# grandchild (e.g. vite under npm, or the API binary under cargo) doesn't get
# reparented and orphaned when its parent dies. Used only in the fallback path.
kill_tree() {
    local pid="$1" child
    for child in $(pgrep -P "$pid" 2>/dev/null); do
        kill_tree "$child"
    done
    kill -TERM "$pid" 2>/dev/null || true
}

cleanup() {
    trap - INT TERM EXIT
    printf '\n>> Shutting down API + web dev server...\n'
    if [ -n "$SCRIPT_PGID" ] && [ "$SCRIPT_PGID" = "$$" ]; then
        # We lead our own group: one signal reaps both servers and their kids.
        kill 0 2>/dev/null || true
    else
        # Not a group leader — signal only our own service subtrees, so we
        # never hit the caller's process group.
        for p in "$API_PID" "$WEB_PID"; do
            [ -n "$p" ] || continue
            kill_tree "$p"
        done
    fi
}
trap cleanup INT TERM EXIT

if [ -n "$CARGO_RELEASE" ]; then
    echo ">> API backend:  http://localhost:3000   (cargo run --release)"
else
    echo ">> API backend:  http://localhost:3000   (cargo run)"
fi
echo ">> Open the app:  http://localhost:5173   (Vite dev server — uses the next free port if 5173 is taken)"
echo ">> Press Ctrl+C to stop both."
echo

# Rust API. The 'if' wrapper keeps a non-zero exit (e.g. a compile error) from
# tripping 'set -e' before we notify the parent, so a crash tears down both.
# $$ inside a subshell is the parent script's PID, which is what we want here.
(
    if cd "$WT/api" && cargo run $CARGO_RELEASE; then ec=0; else ec=$?; fi
    echo ">> [api] cargo run exited (code $ec)"
    kill -TERM $$ 2>/dev/null || true
) &
API_PID=$!

# Web dev server.
(
    if cd "$WT/web" && npm run dev; then ec=0; else ec=$?; fi
    echo ">> [web] dev server exited (code $ec)"
    kill -TERM $$ 2>/dev/null || true
) &
WEB_PID=$!

wait
