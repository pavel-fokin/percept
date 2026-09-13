#!/usr/bin/env bash
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
runs=${RUNS:-1}
conditions=${CONDITIONS:-"bare rails"}
model=${MODEL:-}
codex_bin=${CODEX_BIN:-codex}
out=${OUT:-"$here/runs/$(date +%Y%m%d-%H%M%S)"}

need() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing prerequisite: $1" >&2
    exit 1
  }
}

need "$codex_bin"
need node
need git
need npm
[[ $runs =~ ^[1-9][0-9]*$ ]] || {
  echo "RUNS must be a positive integer: $runs" >&2
  exit 1
}

for condition in $conditions; do
  case "$condition" in
    bare|rails) ;;
    *) echo "unknown condition: $condition (expected bare or rails)" >&2; exit 1 ;;
  esac
done

percept_bin=${PERCEPT_BIN:-}
if [[ $conditions == *rails* ]]; then
  if [[ -z $percept_bin ]]; then
    need cargo
    cargo build --release --manifest-path "$root/Cargo.toml" --quiet
    percept_bin="$root/target/release/percept"
  fi
  [[ -x $percept_bin ]] || {
    echo "PERCEPT_BIN is not executable: $percept_bin" >&2
    exit 1
  }
fi

[[ ! -e $out ]] || {
  echo "output directory already exists: $out" >&2
  exit 1
}
mkdir -p "$out"
summary="$out/summary.tsv"
printf 'condition\trun\ttask\tcodex_exit\tpassed\tfailed\tseconds\tevents\n' >"$summary"

install_test() {
  local repo=$1 stage=$2
  mkdir -p "$repo/test"
  rm -f "$repo/test"/requirement*.test.js
  cp "$here/stages/support.js" "$repo/test/support.js"
  if [[ $stage == 3 ]]; then
    cp "$here/stages/02-resume.test.js" "$repo/test/requirement2.test.js"
    cp "$here/stages/03-shared.test.js" "$repo/test/requirement3.test.js"
  else
    cp "$here/stages/0${stage}-"*.test.js "$repo/test/requirement.test.js"
  fi
}

snapshot_rails() {
  local repo=$1 state=$2 artifacts=$3
  mkdir -p "$artifacts"
  cp "$state/percept.jsonl" "$artifacts/percept.jsonl" 2>/dev/null || :
  (
    cd "$repo"
    PERCEPT_HOME="$state" "$percept_bin" maps list >"$artifacts/maps.txt" 2>"$artifacts/maps.stderr" || :
    PERCEPT_HOME="$state" "$percept_bin" maps show decisions >"$artifacts/decisions.md" 2>>"$artifacts/maps.stderr" || :
    PERCEPT_HOME="$state" "$percept_bin" maps show concepts >"$artifacts/concepts.md" 2>>"$artifacts/maps.stderr" || :
  )
}

run_one() {
  local condition=$1 number=$2
  local run_dir="$out/$condition/run$number"
  local repo="$run_dir/repo"
  local state="$run_dir/percept-home"
  mkdir -p "$repo" "$state"
  cp -R "$here/seed/." "$repo/"

  (
    cd "$repo"
    git init --quiet
    git config user.name "Percept experiment"
    git config user.email "experiment@percept.invalid"

    if [[ $condition == rails ]]; then
      cp "$here/stages/rails-AGENTS.md" AGENTS.md
      PATH="$(dirname "$percept_bin"):$PATH" PERCEPT_HOME="$state" \
        "$percept_bin" init codex --capture >"$run_dir/init.txt"
    fi

    git add -A
    git commit --quiet -m "chore: seed browser chat"
  )

  local stage task_dir codex_status test_status elapsed passed failed events
  for stage in 1 2 3; do
    task_dir="$run_dir/task$stage"
    mkdir -p "$task_dir"
    install_test "$repo" "$stage"
    (
      cd "$repo"
      git add test
      git commit --quiet -m "test: introduce requirement $stage"
    )

    local started=$SECONDS
    codex_status=0
    if [[ -n $model ]]; then
      set -- -m "$model"
    else
      set --
    fi
    PATH="${percept_bin:+$(dirname "$percept_bin"):}$PATH" PERCEPT_HOME="$state" \
      "$codex_bin" exec --ephemeral --approve-for-me --sandbox workspace-write \
      --dangerously-bypass-hook-trust -C "$repo" --json \
      -o "$task_dir/last-message.txt" "$@" - \
      <"$here/stages/0${stage}-"*.md \
      >"$task_dir/codex.jsonl" 2>"$task_dir/codex.stderr" || codex_status=$?
    elapsed=$((SECONDS - started))

    install_test "$repo" "$stage"
    test_status=0
    (cd "$repo" && npm test) >"$task_dir/tests.tap" 2>"$task_dir/tests.stderr" || test_status=$?
    passed=$(awk '/^# pass / {print $3}' "$task_dir/tests.tap" | tail -1)
    failed=$(awk '/^# fail / {print $3}' "$task_dir/tests.tap" | tail -1)
    passed=${passed:-0}
    failed=${failed:-0}
    if [[ $test_status -ne 0 && $failed -eq 0 ]]; then
      failed=1
    fi
    cp "$here/stages/0${stage}-"*.md "$task_dir/prompt.md"
    (cd "$repo" && git status --short >"$task_dir/status.txt" && git diff >"$task_dir/diff.patch")

    if [[ $condition == rails ]]; then
      snapshot_rails "$repo" "$state" "$task_dir/rails"
      events=$(wc -l <"$state/percept.jsonl" 2>/dev/null | tr -d ' ' || echo 0)
    else
      events=0
    fi
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
      "$condition" "$number" "$stage" "$codex_status" "$passed" "$failed" "$elapsed" "$events" \
      | tee -a "$summary"

    (
      cd "$repo"
      git add -A
      git commit --quiet --allow-empty -m "experiment: finish requirement $stage"
    )
  done
}

for condition in $conditions; do
  for number in $(seq 1 "$runs"); do
    run_one "$condition" "$number"
  done
done

echo "results: $out"
echo "summary: $summary"
