#!/usr/bin/env python3
"""List every comment on a Notion page: page-level and inline, including nested and paginated blocks.

Usage (from the repo root): python3 .claude/skills/rfc/comments.py <page-id>
Prints each commented block's id, type and text, the discussion_id to reply to, and each comment.
"""
import json, subprocess, sys

N = ".claude/skills/notion/ntn.sh"

def api(*args):
    r = subprocess.run([N, "api", *args], stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=60)
    return json.loads(r.stdout)

def children(block_id):
    out, cursor = [], None
    while True:
        args = [f"v1/blocks/{block_id}/children", "page_size==100"] + ([f"start_cursor=={cursor}"] if cursor else [])
        d = api(*args)
        out += d["results"]
        if not d.get("has_more"):
            return out
        cursor = d["next_cursor"]

def walk(block_id):
    for b in children(block_id):
        yield b
        if b.get("has_children"):
            yield from walk(b["id"])

def text(b):
    return "".join(r["plain_text"] for r in b[b["type"]].get("rich_text", []))

def show(target_id, label):
    comments = api("v1/comments", f"block_id=={target_id}").get("results", [])
    if comments:
        print(f"\n{label}\n  discussion_id: {comments[0]['discussion_id']}")
        for c in comments:
            print("  -", "".join(t["plain_text"] for t in c["rich_text"]))
    return bool(comments)

if __name__ == "__main__":
    page = sys.argv[1]
    show(page, "[page-level]")
    n = found = 0
    for b in walk(page):
        n += 1
        found += show(b["id"], f"[{b['type']} {b['id']}] {text(b)[:160]}")
    print(f"\nblocks checked: {n}, blocks with comments: {found}")
