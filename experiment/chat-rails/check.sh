#!/usr/bin/env bash
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/percept-chat-rails.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

bash -n "$here/run.sh"

for stage in 1 2 3; do
  repo="$tmp/stage$stage"
  mkdir -p "$repo/test"
  cp -R "$here/seed/." "$repo/"
  cp "$here/reference/server.js" "$repo/server.js"
  cp "$here/reference/app.js" "$repo/public/app.js"
  cp "$here/stages/support.js" "$repo/test/support.js"
  if [[ $stage == 3 ]]; then
    cp "$here/stages/02-resume.test.js" "$repo/test/requirement2.test.js"
    cp "$here/stages/03-shared.test.js" "$repo/test/requirement3.test.js"
  else
    cp "$here/stages/0${stage}-"*.test.js "$repo/test/requirement.test.js"
  fi
  CHAT_REFERENCE_STAGE=$stage node --test "$repo/test"/*.test.js
done

repo="$tmp/seed"
mkdir -p "$repo/test"
cp -R "$here/seed/." "$repo/"
cp "$here/stages/support.js" "$repo/test/support.js"
cp "$here/stages/01-connected.test.js" "$repo/test/requirement.test.js"
if (cd "$repo" && npm test >/dev/null 2>&1); then
  echo "seed unexpectedly passes the first requirement" >&2
  exit 1
fi

echo "chat-rails fixture checks passed"
