#!/usr/bin/env bash
# Publishes the commits on local main when this shared checkout cannot merge
# the remote, because another session has uncommitted work in files the
# incoming commits touch. Nothing in the checkout moves: no merge, no stash,
# no branch switch. It
#
#   1. test-merges HEAD with origin/main in memory and stops on a conflict,
#   2. exports the merged tree to a scratch folder and runs the checks there,
#   3. writes the merge as a commit object and pushes it to origin (GitLab and
#      GitHub), then confirms both heads.
#
# The checkout fast-forwards later, once it is clean.
#
#   scripts/dev/publish-merged.sh [--frontend] [--rust] [--scripts] "what this publishes"
#
# With no check named it runs all three. Name only the ones your commits can
# break, and say so in the summary line.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO"

checks=()
while [[ "${1:-}" == --* ]]; do
  case "$1" in
    --frontend|--rust|--scripts) checks+=("${1#--}") ;;
    *) echo "Unknown option $1" >&2; exit 2 ;;
  esac
  shift
done
[[ ${#checks[@]} -gt 0 ]] || checks=(frontend scripts rust)
summary="${1:-}"
[[ -n "$summary" ]] || { echo "Give a one-line summary of what this publishes." >&2; exit 2; }
[[ "$(git rev-parse --abbrev-ref HEAD)" == "main" ]] || { echo "Not on main." >&2; exit 1; }

git fetch -q origin
head="$(git rev-parse HEAD)"
remote="$(git rev-parse origin/main)"
if git merge-base --is-ancestor "$head" "$remote"; then
  echo "Nothing to publish: origin/main already has every local commit."
  exit 0
fi

if ! tree="$(git merge-tree --write-tree "$head" "$remote")"; then
  echo "The remote does not merge cleanly with local main. Conflicts:" >&2
  echo "$tree" >&2
  exit 1
fi

scratch="${TMPDIR:-/tmp}/fndr-publish"
rm -rf "$scratch"
mkdir -p "$scratch/dist"
trap 'rm -rf "$scratch"' EXIT
git archive "$tree" | tar -x -C "$scratch"
ln -s "$REPO/node_modules" "$scratch/node_modules"

ran=()
for check in "${checks[@]}"; do
  case "$check" in
    frontend) (cd "$scratch" && npm run -s typecheck && npx vitest run >/dev/null) && ran+=("typecheck and frontend tests") ;;
    scripts) (cd "$scratch" && make -s scripts-test >/dev/null 2>&1) && ran+=("script tests") ;;
    rust) (cd "$scratch/src-tauri" && CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" cargo test --lib >/dev/null 2>&1) && ran+=("library tests") ;;
  esac || { echo "The $check check failed on the merged tree. Nothing was pushed." >&2; exit 1; }
done
verified="$(IFS=,; echo "${ran[*]}" | sed 's/,/, /g')"

if git merge-base --is-ancestor "$remote" "$head"; then
  commit="$head"
else
  commit="$(git commit-tree "$tree" -p "$head" -p "$remote" -m "Merge remote main: $summary

Built without moving the shared checkout. Verified on an export of this tree: $verified.")"
fi
git push origin "${commit}:refs/heads/main"
echo "Published $(git rev-parse --short "$commit") (verified: $verified)."
for name in origin gh; do
  git remote get-url "$name" >/dev/null 2>&1 && echo "$name main: $(git ls-remote "$name" main | cut -c1-7)"
done
