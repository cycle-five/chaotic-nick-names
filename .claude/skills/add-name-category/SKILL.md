---
name: add-name-category
description: >-
  Add a new built-in/default nickname category to the chaotic-nick-names Discord
  bot. Use this whenever the user wants to add, create, expand, or "catalog" a
  default category of names in this repo — e.g. "add a category of firearms",
  "new NSFW category for X", "make a list of Y to use as nicknames", "add Z to
  the default categories". Covers settling the curation decisions, building the
  list, splicing it into src/categories.json with a validating script, flagging
  it NSFW in src/data.rs, and verifying with cargo test. Reach for it even if the
  user doesn't say the word "category" but is clearly adding a pool of names the
  bot can randomly assign.
---

# Adding a built-in name category

This bot assigns random Discord nicknames from built-in *categories*. Adding one
is almost entirely a **data-curation** task: the mechanics are two small edits
and the existing tests do the enforcement. The hard part is building a good list.

## Where the pieces live

- **`src/categories.json`** — the data. A pretty-printed (2-space indent) map of
  `{ "slug": ["Name1", "Name2", ...] }`, embedded into the binary at compile
  time via `include_str!` in `src/data.rs`. Category keys are sorted only for
  *display* (`builtin_category_names()` sorts a copy), so the key order in the
  file is cosmetic — but keep it alphabetical for a tidy diff.
- **`src/data.rs`** — `pub const NSFW: &[&str]` lists the slugs that are 18+.
  NSFW categories stay in the catalog and work on explicit request
  (`/randomize category:<slug>`) and show under 🔞 in `/categories`, but are
  excluded from the default random pool (see `randomize::pick_random_category`).
- **`scripts/splice_category.py`** (bundled with this skill) — validates a
  curated list against every invariant below and inserts it into
  categories.json with a clean diff. Use it; don't hand-edit hundreds of lines.

## Invariants the tests already enforce (so you don't write new tests)

The tests in `src/data.rs` will fail the build if any of these are violated.
The bundled script checks the same things up front so you catch them in one pass:

- Every name is **≤ 32 chars** (`chars().count()` — Discord's nickname limit).
- **No leading/trailing whitespace; no empty names.**
- **No duplicate names within a category.**
- **Every `NSFW` entry must be a real category** key in categories.json.
- Category slug should match `^[a-z][a-z0-9_]*$` (lowercase, starts with a
  letter) — this mirrors `valid_category_key` used for user-added categories.

## Dataset conventions (match the existing data, not just the tests)

The tests don't enforce these, but the existing ~19 categories follow them and
breaking the pattern reads as sloppy:

- **No `&` and no apostrophes** anywhere. Spell out "and", or abbreviate
  (e.g. `Smith Wesson`, `HK MP5`). **Avoid periods too** (`Vz 58`, not `Vz. 58`).
- **Hyphens and spaces are fine** (`AK-47`, `Brown Bess`, `Lee-Enfield`).
- These are *nicknames*. Each entry should read as recognizable on its own.

## Settle these decisions before curating

Curation quality depends on a few choices. Ask the user (one quick batch is
fine) unless the request already answers them:

1. **Entry scope** — specific named models only, or also broad historical
   types/classes? (For the firearms category we included both: `Arquebus` the
   class *and* `Colt Python` the model.)
2. **Naming format** — "recognizable form" is usually best: keep a
   manufacturer/qualifier prefix only when the bare designation would be cryptic
   (`Colt Python`, `Beretta 92`) but go bare where iconic (`Luger`, `Garand`).
   Avoid listing the same item under two aliases (pick `Tommy Gun` *or*
   `Thompson`, not both) — they'd survive the dup test but waste pool slots.
3. **Size / ambition** — existing categories range ~13 to ~2000 names. A truly
   "complete" catalog of anything large is impossible to hand-curate accurately;
   be honest that "complete" realistically means "broad and representative."
   ~300–600 well-known entries is a solid, accurate default. Say so rather than
   padding with obscure, possibly-wrong entries.

## Procedure

1. **Pick the slug** (lowercase, underscores), and decide NSFW or not.
2. **Curate the list** per the decisions above. Write the names to a plain text
   file, **one per line** (blank lines and `#` comments are ignored). Group with
   comment headers while drafting — it keeps a long list reviewable and makes
   gaps obvious.
3. **Validate + splice** with the bundled script. Dry-run first to see the
   report and insert point without writing:
   ```bash
   python .claude/skills/add-name-category/scripts/splice_category.py \
       --slug <slug> --names /tmp/<slug>.txt --dry-run
   # then, if happy:
   python .claude/skills/add-name-category/scripts/splice_category.py \
       --slug <slug> --names /tmp/<slug>.txt
   ```
   It refuses to run if the slug already exists, prints the count, and confirms
   the file still parses as JSON afterward.
4. **If NSFW**, add the slug to the `NSFW` const in `src/data.rs`, keeping it
   alphabetical:
   ```rust
   pub const NSFW: &[&str] = &["<slug>", "serial_killers"];
   ```
5. **Verify**: `cargo test`. The data-integrity tests cover correctness — green
   tests mean the category is well-formed and (if NSFW) correctly flagged. No new
   test code is needed.
6. **Review the diff**: it should be exactly one JSON hunk plus (if NSFW) the
   one-line const change. `git diff --stat` is a fast sanity check.
7. **Ship** (only when the user asks): per the repo's deploy flow — bump
   `Cargo.toml`, squash-merge to `master` (which builds the release + GHCR
   image), then `docker --context proxmox compose pull && up -d`.

## Worked example

The `firearms` category (NSFW, 418 entries spanning Hand Cannon → Arquebus →
flintlock muskets → cartridge revolvers → bolt rifles → machine guns → SMGs →
assault rifles → modern pistols) was built exactly this way. Its curation
decisions (entry scope, naming, size) and era coverage are written up in
[`docs/firearms-category.md`](../../../docs/firearms-category.md) — a good
concrete reference for the choices a category like this involves.
