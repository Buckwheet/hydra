#!/bin/sh
# Differential parity check for `hydra replay`.
#
# Runs the ORIGINAL reference binary and this tree's build over every fixture
# and prints a per-kind delta table for each recording that disagrees, then a
# pass/fail summary.  A fixture passes when the two binaries print byte-
# identical stdout *and* exit with the same status.
#
# Usage:
#   tools/diff_replay.sh [--summary|--text] [-v]
#
# Environment:
#   HYDRA_FIXTURES   recording directory  (default: ~/evidence161)
#   HYDRA_ORIGINAL   the reference binary (default: ~/Code/hydra-inspect/hydra)
#   HYDRA_OURS       the binary to test   (default: target/release/hydra)
#
# The fixtures are private recordings: this script reads them in place and
# prints only per-kind counts, never their content.  Exit status: 0 when every
# fixture matches, 1 when any differs, 77 when the check cannot run here.
set -u

mode=--summary
verbose=0
for arg in "$@"; do
    case "$arg" in
        --summary|--text) mode="$arg" ;;
        -v|--verbose) verbose=1 ;;
        -h|--help) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "usage: $0 [--summary|--text] [-v]" >&2; exit 2 ;;
    esac
done

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
fixtures=${HYDRA_FIXTURES:-$HOME/evidence161}
original=${HYDRA_ORIGINAL:-$HOME/Code/hydra-inspect/hydra}
ours=${HYDRA_OURS:-$root/target/release/hydra}
work=${TMPDIR:-/tmp}/hydra-diff-replay.$$

cleanup() { rm -rf "$work"; }
trap cleanup EXIT INT TERM
mkdir -p "$work" || exit 77

if [ ! -d "$fixtures" ]; then
    echo "skip: no fixture directory at $fixtures (set HYDRA_FIXTURES)" >&2
    exit 77
fi
if [ ! -x "$original" ]; then
    echo "skip: no reference binary at $original (set HYDRA_ORIGINAL)" >&2
    exit 77
fi
if [ ! -x "$ours" ]; then
    echo "skip: $ours is not built (cargo build --offline --release -p hydra)" >&2
    exit 77
fi

find "$fixtures" -type f | LC_ALL=C sort > "$work/list"
total=$(grep -c '' "$work/list")

# The kinds (or unknown tags) a summary reports: "{count:>8}  {name}" rows.
counts_of() {
    awk '
        /^== / { next }
        NF == 2 && $1 ~ /^[0-9]+$/ { print $2, $1 }
    ' "$1" | LC_ALL=C sort
}

report_delta() {
    counts_of "$1" > "$work/o.counts"
    counts_of "$2" > "$work/m.counts"
    printf '   %-24s %10s %10s %10s\n' kind original ours delta
    awk '
        NR == FNR { o[$1] = $2; seen[$1] = 1; next }
        { m[$1] = $2; seen[$1] = 1 }
        END {
            for (k in seen) {
                a = (k in o) ? o[k] : 0
                b = (k in m) ? m[k] : 0
                if (a != b)
                    printf "   %-24s %10d %10d %+10d\n", k, a, b, b - a
            }
        }
    ' "$work/o.counts" "$work/m.counts" | LC_ALL=C sort
    if [ "$verbose" -eq 1 ]; then
        diff -u "$1" "$2" | sed -n '1,60p'
    fi
}

ok=0
bad=0
while IFS= read -r file; do
    tag=$(printf '%s' "$file" | cksum | cut -d' ' -f1)
    "$original" replay "$file" "$mode" > "$work/o.$tag" 2> "$work/o.$tag.err"
    o_rc=$?
    "$ours" replay "$file" "$mode" > "$work/m.$tag" 2> "$work/m.$tag.err"
    m_rc=$?
    if [ "$o_rc" -eq "$m_rc" ] && cmp -s "$work/o.$tag" "$work/m.$tag"; then
        ok=$((ok + 1))
        continue
    fi
    bad=$((bad + 1))
    printf '\n== %s (exit %d vs %d)\n' "$file" "$o_rc" "$m_rc"
    report_delta "$work/o.$tag" "$work/m.$tag"
done < "$work/list"

printf '\n%s: %d of %d fixtures identical (%d differing)\n' \
    "$mode" "$ok" "$total" "$bad"
[ "$bad" -eq 0 ]
