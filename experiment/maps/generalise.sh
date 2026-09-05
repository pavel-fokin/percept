#!/usr/bin/env bash
# Does a map make reasoning cheap for questions it was not built for?
# See README.md, "One map, many questions".
#
# Plants four decisions and one open question in a fresh log, records
# the decisions map from the shell so every condition sees the same
# map, buries it all under more events than the model's window holds,
# and asks five questions. Each question runs in its own fresh log, so
# one question's searches never sit in the window of the next.
#
#   bare       no map; the model has to search the log
#   prompt     the whole map in the prompt (today's default)
#   headlines  question and decision nodes in the prompt, read_map tool
#   tool       map name and size in the prompt, read_map tool
#
# The conditions other than bare differ only in PERCEPT_MAPS.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
runs=${RUNS:-3}
conditions=${CONDITIONS:-"bare prompt headlines tool"}
questions=${QUESTIONS:-"q1 q2 q3 q4 q5"}
out="$here/runs/$(date +%Y%m%d-%H%M%S)-generalise"
burial=${BURIAL:-30}

# --- what is planted -----------------------------------------------------

facts=(
  'Decision on ollama connection retries: we weighed 3, 5, and 7 attempts. 3 gave up while a model was still loading and 5 was borderline. We settled on 7 retries, with a two-second pause between them.'
  'Decision on the log format: we weighed SQLite, JSONL, and Postgres. SQLite needed a schema migration story and Postgres needed a server. We settled on JSONL, one event per line, append-only.'
  'Decision on the context window: we weighed 10, 20, and 50 events. 50 overran the 4k context on gemma and 10 lost the question mid tool round. We settled on 20 events.'
  'Decision on the model in tests: we weighed mocking HTTP, a fake Model, and calling ollama. Calling ollama made the tests flaky. We settled on a fake Model in the test module.'
  'Open question on the TUI theme: dark or light by default? Not decided yet. Leaning dark because most terminals are.'
  'Side note on the retries: the two-second pause was Marek'"'"'s idea, after the 3-retry test hung his laptop.'
)

# One question per line: id, what is asked, and what a correct reply must
# contain - every pattern, separated by ;;.
#   q1  the question the map was built for
#   q2  cross-cutting: two decisions share a subject
#   q3  negative: why an option lost
#   q4  open: what has no decision yet
#   q5  only the log holds it: the map has no such node
question() {
  case $1 in
    q1) echo 'How many retries did we settle on for the ollama connection, and why?' ;;
    q2) echo 'Which decisions did we take that involve ollama? Name each.' ;;
    q3) echo 'Why did we not go with 5 retries for the ollama connection?' ;;
    q4) echo 'What is still undecided?' ;;
    q5) echo 'Whose idea was the two-second pause between retries?' ;;
  esac
}
answer() {
  case $1 in
    q1) echo '7|seven' ;;
    q2) echo 'retr;;tests?|fake' ;;
    q3) echo 'borderline' ;;
    q4) echo 'theme|dark|light' ;;
    q5) echo 'Marek' ;;
  esac
}

# --- setup ---------------------------------------------------------------

cargo build --release --manifest-path "$root/Cargo.toml" -q
percept="$root/target/release/percept"

provider=${PERCEPT_PROVIDER:-ollama}
if [ "$provider" = ollama ] && ! curl -s -m 2 localhost:11434/api/tags >/dev/null; then
  echo "ollama is not answering on localhost:11434" >&2
  exit 1
fi
command -v jq >/dev/null || { echo "jq is required" >&2; exit 1; }

say() { "$percept" events publish --actor user --source experiment --type message.received --payload "$(printf '{"content":%s}' "$(jq -Rn --arg c "$1" '$c')")"; }

# Plants every fact; prints the planted events' ids, one per line, in
# the order of `facts`.
plant() {
  for fact in "${facts[@]}"; do
    say "$fact"
    "$percept" events search --size 1 | jq -r .id
  done
}

bury() {
  for i in $(seq 1 "$burial"); do
    say "Note $i: unrelated chatter about the weather, lunch, and a bike ride."
  done
}

node() { "$percept" maps add-node decisions --kind "$1" --name "$2" --source "$3" >/dev/null; }
edge() { "$percept" maps add-edge decisions --kind "$1" --from "$2" --to "$3" --source "$4" >/dev/null; }

# The map a person would record after reading the four decisions and
# the open question. The side note is left out on purpose: q5 asks
# for what only the log holds.
shell_map() {
  local retries=$1 format=$2 window=$3 tests=$4 theme=$5

  node question 'How many ollama retries?' "$retries"
  node option '3 retries' "$retries"
  node option '5 retries' "$retries"
  node option '7 retries' "$retries"
  node evidence '3 gave up while a model was still loading' "$retries"
  node evidence '5 was borderline' "$retries"
  node decision '7 retries with a two-second pause' "$retries"
  edge contradicts 'evidence:3 gave up while a model was still loading' 'option:3 retries' "$retries"
  edge contradicts 'evidence:5 was borderline' 'option:5 retries' "$retries"
  edge supports 'option:7 retries' 'decision:7 retries with a two-second pause' "$retries"
  edge resolves 'decision:7 retries with a two-second pause' 'question:How many ollama retries?' "$retries"

  node question 'Which log format?' "$format"
  node option 'SQLite' "$format"
  node option 'JSONL' "$format"
  node option 'Postgres' "$format"
  node evidence 'SQLite needed a schema migration story' "$format"
  node evidence 'Postgres needed a server' "$format"
  node decision 'JSONL, one event per line, append-only' "$format"
  edge contradicts 'evidence:SQLite needed a schema migration story' 'option:SQLite' "$format"
  edge contradicts 'evidence:Postgres needed a server' 'option:Postgres' "$format"
  edge supports 'option:JSONL' 'decision:JSONL, one event per line, append-only' "$format"
  edge resolves 'decision:JSONL, one event per line, append-only' 'question:Which log format?' "$format"

  node question 'How many events in the context window?' "$window"
  node option '10 events' "$window"
  node option '20 events' "$window"
  node option '50 events' "$window"
  node evidence '50 overran the 4k context on gemma' "$window"
  node evidence '10 lost the question mid tool round' "$window"
  node decision '20 events in the window' "$window"
  edge contradicts 'evidence:50 overran the 4k context on gemma' 'option:50 events' "$window"
  edge contradicts 'evidence:10 lost the question mid tool round' 'option:10 events' "$window"
  edge supports 'option:20 events' 'decision:20 events in the window' "$window"
  edge resolves 'decision:20 events in the window' 'question:How many events in the context window?' "$window"

  node question 'Which model in tests?' "$tests"
  node option 'mock HTTP' "$tests"
  node option 'a fake Model' "$tests"
  node option 'call ollama' "$tests"
  node evidence 'calling ollama made the tests flaky' "$tests"
  node decision 'a fake Model in the test module' "$tests"
  edge contradicts 'evidence:calling ollama made the tests flaky' 'option:call ollama' "$tests"
  edge supports 'option:a fake Model' 'decision:a fake Model in the test module' "$tests"
  edge resolves 'decision:a fake Model in the test module' 'question:Which model in tests?' "$tests"

  node question 'TUI theme: dark or light by default?' "$theme"
  node option 'dark' "$theme"
  node option 'light' "$theme"
  node evidence 'most terminals are dark' "$theme"
  edge supports 'evidence:most terminals are dark' 'option:dark' "$theme"
}

# --- one run -------------------------------------------------------------

# Asks one question under one condition in a fresh log. Prints one row.
run_one() {
  local condition=$1 q=$2 n=$3 dir="$out/$condition/$q/run$n"
  mkdir -p "$dir"
  pushd "$dir" >/dev/null
  # A fresh log per run: percept writes to $PERCEPT_HOME, not the cwd.
  export PERCEPT_HOME="$dir"

  local ids
  ids=$(plant)
  local shape=prompt
  if [ "$condition" != bare ]; then
    # shellcheck disable=SC2046
    shell_map $(echo "$ids" | head -5)
    shape=$condition
  fi
  bury
  "$percept" maps show decisions >map.jsonl

  local t0=$SECONDS status=0
  PERCEPT_MAPS=$shape "$percept" ask "$(question "$q")" >reply.txt 2>trace.txt || status=$?
  local seconds=$((SECONDS - t0))

  local correct=yes pattern
  while IFS= read -r pattern; do
    grep -Eiq "$pattern" reply.txt || correct=no
  done < <(answer "$q" | sed 's/;;/\n/g')

  local calls reads tokens
  calls=$(grep -cE '^⚒ [a-z_]+\(' trace.txt || true)
  reads=$(grep -cE '^⚒ read_map\(' trace.txt || true)
  # Prompt and completion tokens over every model call of the turn.
  tokens=$(jq -rs '[.[] | select(.type == "model.called") | .payload]
    | "\(map(.input_tokens) | add // 0) \(map(.output_tokens) | add // 0) \(map(.cached_tokens // 0) | add // 0)"' percept.jsonl)

  # shellcheck disable=SC2086
  printf '%-10s %-4s %-5s %-8s %-6s %-6s %-8s %-8s %-8s %-6s %s\n' \
    "$condition" "$q" "run$n" "$correct" "$calls" "$reads" $tokens "${seconds}s" "exit $status" | tee summary.txt
  popd >/dev/null
}

# --- main ----------------------------------------------------------------

mkdir -p "$out"
{
  echo "percept maps generalisation experiment - $(date)"
  echo "provider: $provider, runs per cell: $runs, burial: $burial events"
  echo "conditions: $conditions; questions: $questions"
  echo
  printf '%-10s %-4s %-5s %-8s %-6s %-6s %-8s %-8s %-8s %-6s %s\n' condition q run correct calls reads in out cached time status
} | tee "$out/summary.txt"

for condition in $conditions; do
  for q in $questions; do
    for n in $(seq 1 "$runs"); do
      run_one "$condition" "$q" "$n" | tail -1 | tee -a "$out/summary.txt"
    done
  done
done

echo | tee -a "$out/summary.txt"
printf '%-10s %-8s %-8s %-8s %-8s %-8s\n' condition correct calls in out cached | tee -a "$out/summary.txt"
for condition in $conditions; do
  grep "^$condition " "$out/summary.txt" | awk -v c="$condition" '
    { total++; if ($4 == "yes") right++; calls += $5; in_ += $7; out += $8; cached += $9 }
    END { printf "%-10s %-8s %-8d %-8d %-8d %-8d\n", c, right "/" total, calls, in_, out, cached }' \
    | tee -a "$out/summary.txt"
done
echo | tee -a "$out/summary.txt"
echo "per question:" | tee -a "$out/summary.txt"
for q in $questions; do
  for condition in $conditions; do
    grep -E "^$condition +$q " "$out/summary.txt" | awk -v c="$condition" -v q="$q" '
      { total++; if ($4 == "yes") right++; calls += $5 }
      END { printf "  %-4s %-10s %s correct, %d calls\n", q, c, right "/" total, calls }' \
      | tee -a "$out/summary.txt"
  done
done
echo | tee -a "$out/summary.txt"
echo "details in $out"
