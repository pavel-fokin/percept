#!/usr/bin/env bash
# Does a map answer questions it was not built for? See README.md.
#
# Plants four decisions in a fresh log, one of them still open, lets
# the model build a map from them once, buries them, then asks a set of
# questions the map was not written around:
#
#   bare      no map - the model has to search the log
#   reflect   a map the model built itself with `percept reflect`
#
# Each condition runs RUNS times (default 3). A run prepares one log,
# then copies it per question, so an answer never sits in the log the
# next question reads. Everything lands under runs/<timestamp>/.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
runs=${RUNS:-3}
conditions=${CONDITIONS:-"bare reflect"}
out="$here/runs/$(date +%Y%m%d-%H%M%S)"
# How many events to pile on top. The model reads the newest 20.
burial=${BURIAL:-30}
# How often to retry a reflect that built nothing, each time from a
# fresh log. The model sometimes plans the whole call in its thinking
# and then ends its turn, or claims in its reply to have written what
# it never did; a retry in the same log would read that claim and
# believe it.
reflect_tries=${REFLECT_TRIES:-3}

# The planted facts: three decisions and one open question.
facts=(
  'Decision on ollama connection retries: we weighed 3, 5, and 7 attempts. 3 gave up while a model was still loading and 5 was borderline. We settled on 7 retries, with a two-second pause between them.'
  'Decision on the log format: we weighed SQLite and JSONL. SQLite needs a schema migration for every new event kind and cannot be read with grep. We settled on JSONL, one event per line, append-only.'
  'Decision on the TUI library: we weighed cursive and ratatui. cursive owns the event loop, which fights async. We settled on ratatui on top of crossterm.'
  'Open question: should the map in the prompt be capped? Options are no cap, a cap by node count, and a cap by token budget. There is no evidence yet on how big a map gets, so nothing is decided.'
)

# The questions, none of them the one a map is built around, and the
# terms a correct reply must all contain (case-insensitive regexes,
# separated by ;).
questions=(
  'direct|How many retries did we settle on for the ollama connection?|7|seven'
  'cross|List every decision we have taken so far, one line each.|7|seven;jsonl;ratatui'
  'negative|Why did we not go with 5 retries?|borderline'
  'negative2|Why did we reject SQLite for the log?|migration|grep'
  'open|Which question is still undecided?|map;cap|size|limit'
)

cargo build --release --manifest-path "$root/Cargo.toml" -q
percept="$root/target/release/percept"

if ! curl -s -m 2 localhost:11434/api/tags >/dev/null; then
  echo "ollama is not answering on localhost:11434" >&2
  exit 1
fi

# --- helpers -------------------------------------------------------------

say() { "$percept" events publish --actor user --source experiment --type message.received --payload "$(printf '{"content":%s}' "$(jq -Rn --arg c "$1" '$c')")"; }

bury() {
  for i in $(seq 1 "$burial"); do
    say "Note $i: unrelated chatter about the weather, lunch, and a bike ride."
  done
}

plant() {
  rm -f percept.jsonl
  for fact in "${facts[@]}"; do say "$fact" >/dev/null; done
}

node_count() { "$percept" maps show decisions | jq -s '[.[] | select(.node)] | length'; }
cited_count() { "$percept" maps show decisions | jq -s '[.[] | select(.node) | select(.sources | length > 0)] | length'; }

# Reflects until the map holds something or the tries run out, each
# try on a freshly planted log. Leaves reflect-<try>-reply.txt and
# reflect-<try>-trace.txt per attempt and reflect.txt with a one-line
# account.
reflect() {
  local try=1 t0=$SECONDS
  while :; do
    "$percept" reflect >"reflect-$try-reply.txt" 2>"reflect-$try-trace.txt" || true
    [[ $(node_count) -gt 0 || $try -ge $reflect_tries ]] && break
    try=$((try + 1))
    plant
  done
  echo "tries $try, $((SECONDS - t0))s, nodes $(node_count), cited $(cited_count)" >reflect.txt
}

# Whether reply.txt carries every term of a `;`-separated regex list.
answers() {
  local terms=$1 term
  IFS=';' read -ra term <<<"$terms"
  for t in "${term[@]}"; do
    grep -Eiq "$t" reply.txt || return 1
  done
}

# One run of one condition: prepare the log once, then ask each
# question in a copy. Prints one summary row per question.
run_one() {
  local condition=$1 n=$2 dir="$out/$condition/run$n"
  mkdir -p "$dir/base"
  pushd "$dir/base" >/dev/null
  plant
  [[ $condition == reflect ]] && reflect
  bury
  "$percept" maps show decisions >map.jsonl
  local nodes
  nodes=$(node_count)
  popd >/dev/null

  local entry name question terms
  for entry in "${questions[@]}"; do
    IFS='|' read -r name question terms <<<"$entry"
    # `|` also separates alternatives inside a term, so re-split: the
    # first two fields are name and question, the rest is the terms.
    terms=${entry#"$name|$question|"}
    mkdir -p "$dir/$name"
    cp "$dir/base/percept.jsonl" "$dir/$name/"
    pushd "$dir/$name" >/dev/null
    local t0=$SECONDS status=0 correct=no calls
    "$percept" ask "$question" >reply.txt 2>trace.txt || status=$?
    answers "$terms" && correct=yes
    calls=$(grep -cE '^⚒ [a-z_]+\(' trace.txt || true)
    printf '%-8s %-5s %-10s %-8s %-6s %-6s %-8s %s\n' \
      "$condition" "run$n" "$name" "$correct" "$calls" "$nodes" "$((SECONDS - t0))s" "exit $status" | tee summary.txt
    popd >/dev/null
  done
}

# --- main ----------------------------------------------------------------

mkdir -p "$out"
{
  echo "percept questions experiment - $(date)"
  echo "runs per condition: $runs, burial: $burial events, conditions: $conditions"
  echo
  printf '%-8s %-5s %-10s %-8s %-6s %-6s %-8s %s\n' condition run question correct calls nodes time status
} | tee "$out/summary.txt"

for condition in $conditions; do
  for n in $(seq 1 "$runs"); do
    run_one "$condition" "$n" | tee -a "$out/summary.txt"
  done
done

echo | tee -a "$out/summary.txt"
for condition in $conditions; do
  total=$(grep -c "^$condition " "$out/summary.txt" || true)
  right=$(grep "^$condition " "$out/summary.txt" | awk '$4=="yes"' | wc -l | tr -d ' ')
  calls=$(grep "^$condition " "$out/summary.txt" | awk '{s+=$5} END {print s+0}')
  echo "$condition: $right/$total correct, $calls tool calls in total" | tee -a "$out/summary.txt"
  for name in direct cross negative negative2 open; do
    right=$(grep "^$condition " "$out/summary.txt" | awk -v q="$name" '$3==q && $4=="yes"' | wc -l | tr -d ' ')
    total=$(grep "^$condition " "$out/summary.txt" | awk -v q="$name" '$3==q' | wc -l | tr -d ' ')
    echo "  $name: $right/$total" | tee -a "$out/summary.txt"
  done
done
if [[ $conditions == *reflect* ]]; then
  echo | tee -a "$out/summary.txt"
  for f in "$out"/reflect/run*/base/reflect.txt; do
    echo "$(basename "$(dirname "$(dirname "$f")")") reflect: $(cat "$f")" | tee -a "$out/summary.txt"
  done
fi
echo | tee -a "$out/summary.txt"
echo "details in $out"
