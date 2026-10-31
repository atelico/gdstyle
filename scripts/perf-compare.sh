#!/usr/bin/env bash
# Compare the cost of two gdstyle binaries on a corpus of GDScript files.
#
# Usage: scripts/perf-compare.sh BASE_BINARY CANDIDATE_BINARY CORPUS_DIR
#
# The gate is the instruction count under valgrind's cachegrind, which barely
# moves between runs, unlike wall-clock time on a shared CI runner. Rayon runs
# on a single thread so idle workers spinning don't add to the count.
# Wall-clock times from hyperfine (when installed) are reported, not gated.
#
# Environment:
#   PERF_THRESHOLD_PERCENT  largest allowed instruction increase (default 3)
#   PERF_ALLOW_REGRESSION   when "1", report a regression but exit 0
#   GITHUB_STEP_SUMMARY     when set, the report is appended to it as well
#
# Example (Linux, valgrind installed):
#   scripts/perf-compare.sh base/target/release/gdstyle \
#       target/release/gdstyle ../godot-demo-projects
set -euo pipefail

if [[ $# -ne 3 ]]; then
    echo "usage: $0 BASE_BINARY CANDIDATE_BINARY CORPUS_DIR" >&2
    exit 2
fi

base_binary=$(realpath "$1")
candidate_binary=$(realpath "$2")
corpus_dir=$(realpath "$3")
threshold_percent=${PERF_THRESHOLD_PERCENT:-3}

for binary in "$base_binary" "$candidate_binary"; do
    if [[ ! -x "$binary" ]]; then
        echo "error: '$binary' is not an executable file" >&2
        exit 2
    fi
done
if ! command -v valgrind >/dev/null; then
    echo "error: valgrind is required to count instructions" >&2
    exit 2
fi

# Each entry is the argument list passed to gdstyle, run from the corpus
# directory so config lookup never reaches a gdstyle.toml above it.
benchmarks=("check ." "fmt --check .")

# Print the instruction count of one gdstyle run.
count_instructions() {
    local binary=$1 arguments=$2 log status
    log=$(mktemp)
    set +e
    # shellcheck disable=SC2086 # the arguments are meant to split into words
    (cd "$corpus_dir" && RAYON_NUM_THREADS=1 valgrind --tool=cachegrind \
        --cache-sim=no --cachegrind-out-file=/dev/null \
        "$binary" $arguments >/dev/null 2>"$log")
    status=$?
    set -e
    # gdstyle exits 1 when it finds something to report. Anything else means
    # the run itself failed, and its count would compare nothing.
    if [[ $status -ne 0 && $status -ne 1 ]]; then
        echo "error: '$binary $arguments' exited with $status" >&2
        cat "$log" >&2
        rm -f "$log"
        exit 2
    fi
    # cachegrind's summary line looks like `==123== I refs:   1,234,567`.
    local instructions
    instructions=$(sed -n 's/.*I *refs: *\([0-9,]*\).*/\1/p' "$log" | tr -d ,)
    rm -f "$log"
    if [[ -z "$instructions" ]]; then
        echo "error: no instruction count in valgrind output for '$binary $arguments'" >&2
        exit 2
    fi
    echo "$instructions"
}

report=$(mktemp)
trap 'rm -f "$report"' EXIT
has_regression=0

{
    echo "### Instruction count (gate: +${threshold_percent}%)"
    echo
    echo "| Command | Base | Candidate | Change |"
    echo "|---|---:|---:|---:|"
} >>"$report"

for arguments in "${benchmarks[@]}"; do
    base_count=$(count_instructions "$base_binary" "$arguments")
    candidate_count=$(count_instructions "$candidate_binary" "$arguments")
    change_percent=$(awk -v base="$base_count" -v candidate="$candidate_count" \
        'BEGIN { printf "%+.2f", (candidate - base) * 100 / base }')
    marker=""
    if awk -v change="$change_percent" -v limit="$threshold_percent" \
        'BEGIN { exit !(change > limit) }'; then
        marker=" :x:"
        has_regression=1
    fi
    echo "| \`gdstyle $arguments\` | $base_count | $candidate_count | ${change_percent}%${marker} |" >>"$report"
done

if command -v hyperfine >/dev/null; then
    {
        echo
        echo "### Wall-clock time (informational, all threads)"
        echo
    } >>"$report"
    for arguments in "${benchmarks[@]}"; do
        markdown=$(mktemp)
        (cd "$corpus_dir" && hyperfine --style none --warmup 3 --runs 20 \
            --ignore-failure --export-markdown "$markdown" \
            --command-name "base: gdstyle $arguments" "$base_binary $arguments" \
            --command-name "candidate: gdstyle $arguments" "$candidate_binary $arguments" \
            >/dev/null 2>&1)
        cat "$markdown" >>"$report"
        echo >>"$report"
        rm -f "$markdown"
    done
fi

cat "$report"
if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    cat "$report" >>"$GITHUB_STEP_SUMMARY"
fi

if [[ $has_regression -eq 1 ]]; then
    if [[ "${PERF_ALLOW_REGRESSION:-0}" == "1" ]]; then
        echo "Instruction count rose more than ${threshold_percent}%, allowed by the perf-regression-ok label." >&2
        exit 0
    fi
    echo "error: instruction count rose more than ${threshold_percent}% over the base." >&2
    echo "If the cost is intended, add the perf-regression-ok label to the PR." >&2
    exit 1
fi
