#!/usr/bin/env python3
"""Validate a curated name list and splice it into src/categories.json.

Why this exists
---------------
categories.json is a pretty-printed `{ "slug": ["Name", ...] }` map embedded
into the binary at compile time. The Rust tests in src/data.rs enforce a set of
invariants on every name. This script checks those *same* invariants in Python
BEFORE writing, so you find problems in one fast pass instead of via a failing
`cargo test`, and it inserts the new block at its alphabetical key position
while leaving every other byte of the file untouched — a clean, reviewable diff
(re-dumping the whole JSON would reformat and churn all 4000+ lines).

Usage
-----
    # names file: one name per line; blank lines and lines starting with # ignored
    python splice_category.py --slug firearms --names firearms.txt

    # validate + show where it would insert, without writing:
    python splice_category.py --slug firearms --names firearms.txt --dry-run

    # point at a different repo checkout:
    python splice_category.py --slug X --names X.txt --json /path/to/src/categories.json

The NSFW flag is a SEPARATE one-line edit to src/data.rs (add the slug to the
`NSFW` const) — this script only touches categories.json. See the SKILL.md.
"""
import argparse
import json
import re
import sys

SLUG_RE = re.compile(r"^[a-z][a-z0-9_]*$")
KEY_LINE_RE = re.compile(r'^  "([a-z0-9_]+)": \[')
# Mirror src/commands/categories.rs::valid_category_key and the dataset's own
# punctuation habits (no & or apostrophes anywhere in the existing data).
MAX_LEN = 32  # Discord nickname limit (chars, not bytes)
BANNED = ("&", "'")


def load_names(path):
    names = []
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            s = line.rstrip("\r\n")  # strip both LF and CRLF endings, keep other whitespace
            stripped = s.strip()
            if not stripped or stripped.startswith("#"):
                continue
            names.append(s)  # keep as-is so the whitespace check below can catch slop
    return names


def validate(slug, names):
    errors = []
    if not SLUG_RE.match(slug):
        errors.append(f"slug {slug!r} must match ^[a-z][a-z0-9_]*$ (lowercase, starts with a letter)")
    if not names:
        errors.append("name list is empty (a category needs at least one name)")
    seen = {}
    for i, name in enumerate(names):
        if name != name.strip():
            errors.append(f"[{i}] surrounding whitespace: {name!r}")
        if not name.strip():
            errors.append(f"[{i}] empty name")
        if len(name) > MAX_LEN:
            errors.append(f"[{i}] {len(name)} chars > {MAX_LEN} limit: {name!r}")
        for ch in BANNED:
            if ch in name:
                errors.append(f"[{i}] banned punctuation {ch!r} (dataset uses none): {name!r}")
        if name in seen:
            errors.append(f"[{i}] duplicate of [{seen[name]}]: {name!r}")
        else:
            seen[name] = i
    return errors


def render_block(slug, names):
    lines = [f'  "{slug}": [']
    for j, name in enumerate(names):
        comma = "," if j < len(names) - 1 else ""
        lines.append(f"    {json.dumps(name, ensure_ascii=False)}{comma}")
    lines.append("  ],")  # trailing comma is correct because we insert BEFORE a later key
    return "\n".join(lines) + "\n"


def find_anchor(text, slug):
    """Return the existing key line to insert *before* (alphabetical order).

    Returns (anchor_line, None) for the normal "insert before a later key" case,
    or (None, last_close_idx) when the new slug sorts after every existing key
    (append case, handled specially by the caller).
    """
    lines = text.splitlines()
    keys = [(m.group(1), idx) for idx, line in enumerate(lines)
            for m in [KEY_LINE_RE.match(line)] if m]
    for name, idx in keys:
        if name > slug:
            return lines[idx], None
    return None, lines  # append case


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--slug", required=True, help="category key, e.g. firearms")
    ap.add_argument("--names", required=True, help="text file, one name per line (# comments ok)")
    ap.add_argument("--json", default="src/categories.json", help="path to categories.json")
    ap.add_argument("--dry-run", action="store_true", help="validate and report, but do not write")
    args = ap.parse_args()

    names = load_names(args.names)
    errors = validate(args.slug, names)
    if errors:
        print("VALIDATION FAILED:")
        for e in errors:
            print("  " + e)
        sys.exit(1)
    print(f"OK: '{args.slug}' has {len(names)} names — all unique, <= {MAX_LEN} chars, clean punctuation.")

    with open(args.json, "r", encoding="utf-8") as f:
        text = f.read()

    if f'"{args.slug}"' in text:
        print(f"ERROR: key {args.slug!r} already exists in {args.json}; aborting.")
        sys.exit(1)

    block = render_block(args.slug, names)
    anchor, append_lines = find_anchor(text, args.slug)

    if anchor is not None:
        where = anchor.strip().split('"')[1]
        print(f"Insert point: alphabetically before existing key '{where}'.")
        new_text = text.replace(anchor, block + anchor, 1)
    else:
        # New slug sorts after all keys: give the previously-last block a trailing
        # comma and place ours (no trailing comma) just before the closing brace.
        print("Insert point: after all existing keys (append).")
        lines = append_lines
        close_idx = max(i for i, ln in enumerate(lines) if ln.strip() == "]")
        lines[close_idx] = lines[close_idx] + ","
        tail = render_block(args.slug, names).rstrip("\n")
        if tail.endswith("],"):
            tail = tail[:-1]  # drop the comma for the now-last block
        lines.insert(close_idx + 1, tail)
        new_text = "\n".join(lines) + "\n"

    # Always confirm the result is valid JSON and round-trips before committing it.
    data = json.loads(new_text)
    assert data[args.slug] == names, "round-trip mismatch"
    print(f"Result parses: {len(data)} categories; '{args.slug}' = {len(data[args.slug])} names.")

    if args.dry_run:
        print("--dry-run: not writing.")
        return
    with open(args.json, "w", encoding="utf-8") as f:
        f.write(new_text)
    print(f"Wrote {args.json}.")


if __name__ == "__main__":
    main()
