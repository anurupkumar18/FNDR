#!/usr/bin/env bash
# Create the FNDR label set and 12 weekly milestones in GitLab. Safe to run twice.
#
#   GITLAB_TOKEN=<personal access token with api scope> scripts/team/gitlab_bootstrap.sh
#   scripts/team/gitlab_bootstrap.sh --dry-run        # print what would be created, needs no token
#
# Never commit the token. Host and project can be overridden with GITLAB_HOST and GITLAB_PROJECT
# (the project path is URL-encoded, for example fndr%2Ffndr).
set -euo pipefail

HOST="${GITLAB_HOST:-https://capstone.cs.utah.edu}"
PROJECT="${GITLAB_PROJECT:-fndr%2Ffndr}"
DRY_RUN=0
FAILED=0
[[ "${1:-}" == "--dry-run" ]] && DRY_RUN=1

if [[ $DRY_RUN -eq 0 && -z "${GITLAB_TOKEN:-}" ]]; then
  echo "Set GITLAB_TOKEN to a personal access token with the api scope (never commit it)." >&2
  exit 1
fi

api() {
  local path="$1"; shift
  if [[ $DRY_RUN -eq 1 ]]; then
    echo "DRY POST $path $*"
    return 0
  fi
  local code
  code=$(curl -sS -o /dev/null -w '%{http_code}' -X POST \
    -H "PRIVATE-TOKEN: ${GITLAB_TOKEN}" "${HOST}/api/v4/projects/${PROJECT}/${path}" "$@")
  case "$code" in
    200|201) echo "ok   ${path} $*" ;;
    400|409) echo "skip ${path} $* (already exists)" ;;
    *) echo "FAIL ${code} ${path} $*" >&2; FAILED=1 ;;
  esac
}

LABELS=(
  "status::ready|#5BC0DE" "status::doing|#F0AD4E" "status::review|#A78BFA" "status::evidence|#34D399"
  "ws::capture|#1F77B4" "ws::model|#9467BD" "ws::features|#2CA02C" "ws::ops|#7F7F7F"
  "ws::security|#D62728" "ws::learning|#BCBD22"
  "type::feature|#0E8A16" "type::spike|#FBCA04" "type::bug|#B60205" "type::chore|#C5DEF5"
  "type::docs|#0075CA" "type::learning|#D4C5F9"
  "prio::p0|#B60205" "prio::p1|#FBCA04" "prio::p2|#C2E0C6"
  "phase::beta|#1D76DB" "phase::final|#5319E7"
  "evidence::needed|#E99695" "evidence::attached|#0E8A16"
  "ws::memory|#8C564B" "ws::retrieval|#17BECF" "ws::decisions|#E377C2"
  "blocked|#000000" "agent-ok|#006B75" "needs-human|#D93F0B"
  "agent::either|#C2E0C6" "agent::claude|#D2691E" "agent::codex|#4B0082"
)

MILESTONES=(
  "W01-Baseline|2026-09-21|2026-09-27" "W02-Measure|2026-09-28|2026-10-04"
  "W03-Build|2026-10-05|2026-10-11" "W04-Prove|2026-10-12|2026-10-18"
  "W05-Retro|2026-10-19|2026-10-25" "W06-Foundations|2026-10-26|2026-11-01"
  "W07-FineTune|2026-11-02|2026-11-08" "W08-Preference|2026-11-09|2026-11-15"
  "W09-Agent|2026-11-16|2026-11-22" "W10-Harden|2026-11-23|2026-11-29"
  "W11-Freeze|2026-11-30|2026-12-06" "W12-Submit|2026-12-07|2026-12-13"
)

for entry in "${LABELS[@]}"; do
  api labels --data-urlencode "name=${entry%%|*}" --data-urlencode "color=${entry##*|}"
done

for entry in "${MILESTONES[@]}"; do
  IFS='|' read -r title start due <<<"$entry"
  api milestones --data-urlencode "title=${title}" --data "start_date=${start}" --data "due_date=${due}"
done

exit "$FAILED"
