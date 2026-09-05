#!/usr/bin/env bash
# Does a cognitive map change how the model answers? See README.md.
#
# Plants one fact in a fresh log, buries it under more events than the
# model's window holds, and asks about it under three conditions:
#
#   bare      no map - the model has to search the log
#   shell     a map recorded from the shell, citing the planted event
#   reflect   a map the model built itself with `percept reflect`
#
# Each condition runs RUNS times (default 3). Every run gets its own
# directory under runs/<timestamp>/ with the log, the reply, the tool
# trace, and the map, so a result can be read back later.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
runs=${RUNS:-3}
conditions=${CONDITIONS:-"bare shell reflect"}
out="$here/runs/$(date +%Y%m%d-%H%M%S)"

# The planted fact. A number the model cannot guess, in a message the
# window will have lost by the time the question is asked.
fact='Decision on ollama connection retries: we weighed 3, 5, and 7 attempts. 3 gave up while a model was still loading and 5 was borderline. We settled on 7 retries, with a two-second pause between them.'
question='How many retries did we settle on for the ollama connection, and why?'
# What a correct reply must contain.
answer='7|seven'
# How many events to pile on top. The model reads the newest 20.
burial=${BURIAL:-30}

cargo build --release --manifest-path "$root/Cargo.toml" -q
percept="$root/target/release/percept"

provider=${PERCEPT_PROVIDER:-ollama}
if [ "$provider" = ollama ] && ! curl -s -m 2 localhost:11434/api/tags >/dev/null; then
  echo "ollama is not answering on localhost:11434" >&2
  exit 1
fi

# --- helpers -------------------------------------------------------------

# `publish` prints the new event's id; quiet here, so `plant` captures
# exactly one id from its search and `bury` prints nothing.
say() { "$percept" events publish --actor user --source experiment --type message.received --payload "$(printf '{"content":%s}' "$(jq -Rn --arg c "$1" '$c')")" >/dev/null; }

plant() { say "$fact"; "$percept" events search --size 1 | jq -r .id; }

bury() {
  for i in $(seq 1 "$burial"); do
    say "Note $i: unrelated chatter about the weather, lunch, and a bike ride."
  done
}

# The same map a person would record after reading the fact.
shell_map() {
  local seed=$1
  "$percept" maps add-node decisions --kind question --name "How many ollama retries?" --source "$seed" >/dev/null
  "$percept" maps add-node decisions --kind option --name "3 retries" --source "$seed" >/dev/null
  "$percept" maps add-node decisions --kind option --name "5 retries" --source "$seed" >/dev/null
  "$percept" maps add-node decisions --kind option --name "7 retries" --source "$seed" >/dev/null
  "$percept" maps add-node decisions --kind evidence --name "3 gave up while a model was still loading" --source "$seed" >/dev/null
  "$percept" maps add-node decisions --kind decision --name "7 retries with a two-second pause" --source "$seed" >/dev/null
  "$percept" maps add-edge decisions --kind contradicts --from "evidence:3 gave up while a model was still loading" --to "option:3 retries" --source "$seed"
  "$percept" maps add-edge decisions --kind supports --from "option:7 retries" --to "decision:7 retries with a two-second pause" --source "$seed"
  "$percept" maps add-edge decisions --kind resolves --from "decision:7 retries with a two-second pause" --to "question:How many ollama retries?" --source "$seed"
}

# One run of one condition, in its own directory. Prints one summary row.
run_one() {
  local condition=$1 n=$2 dir="$out/$condition/run$n"
  mkdir -p "$dir"
  # Its own repository, so percept takes the run directory as the
  # project and renders maps there, never into this repo's .percept/.
  git init -q "$dir"
  pushd "$dir" >/dev/null
  # A fresh log per run: percept writes to $PERCEPT_HOME, not the cwd.
  export PERCEPT_HOME="$dir"

  local seed
  seed=$(plant)
  case $condition in
    bare) ;;
    shell) shell_map "$seed" ;;
    reflect)
      # The fact is still inside the window here: this measures whether
      # the model can build a map, not whether it can find the fact.
      local t0=$SECONDS
      "$percept" reflect >reflect-reply.txt 2>reflect-trace.txt || true
      echo $((SECONDS - t0)) >reflect-seconds.txt
      ;;
  esac
  bury
  "$percept" maps show decisions >map.jsonl

  local t0=$SECONDS status=0
  "$percept" ask "$question" >reply.txt 2>trace.txt || status=$?
  local seconds=$((SECONDS - t0))

  local correct=no calls found=no nodes cited
  grep -Eiq "$answer" reply.txt && correct=yes
  calls=$(grep -cE '^⚒ [a-z_]+\(' trace.txt || true)
  grep -q "$seed" trace.txt && found=yes
  nodes=$(grep -c '"node"' map.jsonl || true)
  # Nodes citing at least one event, out of nodes built.
  cited=$(jq -s '[.[] | select(.node) | select(.sources | length > 0)] | length' map.jsonl)

  printf '%-8s %-5s %-8s %-6s %-11s %-6s %-8s %-8s %s\n' \
    "$condition" "run$n" "$correct" "$calls" "$found" "$nodes" "$cited/$nodes" "${seconds}s" "exit $status" | tee summary.txt
  popd >/dev/null
}

# --- main ----------------------------------------------------------------

mkdir -p "$out"
{
  echo "percept maps experiment - $(date)"
  echo "provider: $provider, runs per condition: $runs, burial: $burial events, conditions: $conditions"
  echo
  printf '%-8s %-5s %-8s %-6s %-11s %-6s %-8s %-8s %s\n' condition run correct calls found-seed nodes sources time status
} | tee "$out/summary.txt"

for condition in $conditions; do
  for n in $(seq 1 "$runs"); do
    run_one "$condition" "$n" | tail -1 | tee -a "$out/summary.txt"
  done
done

echo | tee -a "$out/summary.txt"
for condition in $conditions; do
  total=$(grep -c "^$condition " "$out/summary.txt" || true)
  right=$(grep "^$condition " "$out/summary.txt" | awk '$3=="yes"' | wc -l | tr -d ' ')
  calls=$(grep "^$condition " "$out/summary.txt" | awk '{s+=$4} END {print s+0}')
  echo "$condition: $right/$total correct, $calls tool calls in total" | tee -a "$out/summary.txt"
done
echo | tee -a "$out/summary.txt"
echo "details in $out"
