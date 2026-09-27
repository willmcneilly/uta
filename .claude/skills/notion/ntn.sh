#!/usr/bin/env bash
# Runs ntn against Will's personal Notion workspace ("Will"), never the default (Stora).
export NOTION_WORKSPACE_ID=d8d40a79-fb78-4b66-8ee4-025778b2ba2e
exec ntn "$@"
