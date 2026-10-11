#!/usr/bin/env bash
# Runs ntn against Will's personal Notion workspace ("Will"), never the default (Stora).
#
#   ntn.sh <ntn args>                   any ntn command: `pages get <id>`, `api v1/comments ...`
#   ntn.sh --stdin <ntn args>           the same, reading a body from stdin (`pages create`, `pages update`, `api`)
#   ntn.sh <source> query [args]        `datasources query` on a named data source (--filter, --sort, --limit, --json)
#   ntn.sh <source> create -d '<json>'  create a row: `parent` is set to the data source for you
#   ntn.sh <source> id                  print the data source ID
#
# Sources: tickets projects rfcs research scratchpad glossary decisions process-log
#
# Stdin is closed unless --stdin comes first: run without a terminal, ntn waits on an open stdin forever.
set -eo pipefail
export NOTION_WORKSPACE_ID=d8d40a79-fb78-4b66-8ee4-025778b2ba2e

source_id() {
  case "$1" in
    rfcs) echo ab1b763c-b9f2-48c5-ade6-a1ba218330f0 ;;
    projects) echo e0dc7875-8865-4bbe-bcff-785efb0cda42 ;;
    tickets) echo 444117ac-b9f1-4686-b828-1919e9ba098f ;;
    research) echo b88b291e-91e7-43f8-896f-68e7084eaa6b ;;
    scratchpad) echo 847d5e5a-0916-4048-b2a0-38722bb92e22 ;;
    glossary) echo c0000cba-8a67-441c-a229-8bdc19e5efb3 ;;
    decisions) echo 795ede6b-3d43-45f7-b39f-dd76bd8ab69c ;;
    process-log) echo 558259e3-1a50-449b-8e99-4a7b66f25f33 ;;
    *) return 1 ;;
  esac
}

if [[ "$1" == --stdin ]]; then
  shift
  exec ntn "$@"
fi

if [[ $# -gt 0 ]] && ds=$(source_id "$1"); then
  name=$1
  shift
  case "$1" in
    query)
      shift
      exec ntn datasources query "$ds" "$@" </dev/null
      ;;
    create)
      if [[ $# -ne 3 || ("$2" != -d && "$2" != --data) ]]; then
        echo "usage: ntn.sh $name create -d '<json>'" >&2
        exit 2
      fi
      body=$(jq -c --arg ds "$ds" '.parent = {data_source_id: $ds}' <<<"$3")
      exec ntn api v1/pages -d "$body" </dev/null
      ;;
    id)
      echo "$ds"
      ;;
    *)
      echo "usage: ntn.sh $name query [args] | create -d '<json>' | id" >&2
      exit 2
      ;;
  esac
  exit 0
fi

exec ntn "$@" </dev/null
