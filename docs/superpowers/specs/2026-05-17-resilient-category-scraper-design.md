# Resilient Category Scraper — Design

**Date:** 2026-05-17
**Status:** Approved (pending spec review)
**Author:** Lothrop + Claude

## 1. Goal

Populate `src/categories.json` with **at least 10 new categories** plus the
**8 existing categories expanded**, targeting **300–500 entries each** where the
subject matter allows. The data feeds a Rust Discord bot that randomly renames
~300 server members, so small lists (current 15–25 each) cause heavy repeats.

`main.py` is rebuilt into a reliable scraper that can be run **tonight** and
cannot leave a category empty.

## 2. Hard Constraints (already enforced by Rust tests in `src/data.rs`)

- Each name ≤ **32 chars** (Discord nickname limit).
- **No duplicate** names within a category.
- No leading/trailing whitespace; no empty names.
- `src/categories.json` must remain valid JSON of shape `{category: [String]}`.
- Names must match `^[A-Za-z0-9 -]+$` after cleaning (ASCII only — the existing
  scraper regex and the bot's expectations).

## 3. Architecture

Single-file Python tool (`main.py`), run via `uv run python main.py`.

### 3.1 Source config table

Replaces the ad-hoc `scrape_*` functions:

```python
SOURCES = {
  "<category>": [
      (url, strategy, options_dict),   # one or more sources per category
      ...
  ],
  ...
}
```

### 3.2 Extraction strategies

All strategies scope to the article body `div.mw-parser-output` and **exclude**
`.navbox`, `.reflist`, `ol.references`, `#toc`, and content under "See also" /
"References" / "External links" / "Notes" headings.

- **`links`** — text of `<a href="/wiki/...">` in body, excluding namespaced
  links (`File:`, `Category:`, `Help:`, `Wikipedia:`, `Template:`, `Portal:`,
  `Special:`). Best for most "List of X" pages.
- **`bullets`** — `<li>` text within body `<ul>`, first segment before
  `–`, `—`, `(`, or ` - `.
- **`table_col`** — column `options["col"]` (default 0) of each
  `table.wikitable` data row; prefers the cell's link text, falls back to cell
  text. For tabular lists (constellations, elements, cars, spices, colors).

### 3.3 Cleaning pipeline (one function, applied to every candidate)

1. `unidecode` transliteration (André→Andre, Lovász→Lovasz).
2. Strip characters not in `^[A-Za-z0-9 -]+$`.
3. Collapse internal whitespace; trim.
4. Reject if `len < 3` or `len > 32`.
5. Reject if in the junk/stopword set (`List`, `Lists`, `Index`, `References`,
   `See also`, `ISBN`, `Wikipedia`, `Category`, `Portal`, `Edit`, `vte`,
   single-letter section headers `A`–`Z`, etc.).
6. Case-insensitive dedupe within the category (keep first-seen casing).

### 3.4 Seed + scrape merge (reliability floor)

Each category has a **curated seed list** committed directly into
`categories.json`. The scraper:

1. Loads existing `categories.json` (seeds live here).
2. For each category, scrapes its `SOURCES`, runs every candidate through the
   cleaning pipeline, and **adds** new clean names to the existing list.
3. **Never removes** seed entries — final count ≥ seed count, guaranteed.
4. Prints a per-category report; emits a loud `⚠️` if final count is below the
   category's `min_target`.

Worst case (all scrapes fail / proxy down): the bot still ships tonight on seeds
alone. Best case: 300–500 per scrapeable category.

### 3.5 Output

Write `src/categories.json` with `json.dumps(..., indent=2,
ensure_ascii=False, sort_keys=True)` (names are ASCII post-clean; sorted keys
match current file + Rust expectations).

## 4. Categories, Sources, and Targets

`min_target` = warn-if-below threshold. Capped categories target their natural
maximum, not 300.

| Category | Seed (committed) | Scrape source(s) | min_target | Notes |
|---|---|---|---|---|
| board_games | ~120 | `List_of_board_games` (links) | 300 | |
| cocktails | ~120 | `List_of_cocktails`, `IBA_official_cocktail` (links/bullets) | 300 | |
| mythical_creatures | ~120 | `List_of_legendary_creatures_(A)`…`(Z)` (links) | 300 | A–Z index pages |
| superheroes | ~120 | `List_of_superheroes` + `List_of_DC/Marvel_Comics_characters` (links) | 300 | |
| cars | ~120 | `List_of_car_brands`, `List_of_automobiles` (links/table) | 300 | brands + models |
| scientists *(existing, expand)* | ~120 | `List_of_physicists/chemists/biologists/astronomers/mathematicians` (links) | 300 | |
| strains_weed | ~120 | `List_of_Cannabis_strains` (links, best-effort) | 200 | seed-primary; Wikipedia weak |
| fictional_villainesses | ~120 | `List_of_female_supervillains` (links, best-effort) | 150 | seed-primary; replaces `woman_murders` |
| hard_things | ~100 | none (curated only) | 100 | def below |
| constellations | 88 | `IAU_designated_constellations` (table_col 0) | 88 | **hard cap = 88** |
| spices | ~120 | `List_of_culinary_herbs_and_spices` (table/links) | 150 | **cap ≈ 150** |
| amusement_parks *(existing, expand)* | ~80 | `List_of_amusement_parks` worldwide (links/table) | 200 | |
| chemical_compounds *(existing, expand)* | ~80 | `List_of_compounds` (links, best-effort) | 150 | seed-primary |
| colors *(existing, expand)* | ~80 | `List_of_colors_(compact)` / A–F,G–M,N–Z (table/links) | 300 | |
| dinosaurs *(existing, expand)* | ~80 | `List_of_dinosaur_genera` (links/table) | 300 | huge source |
| elements *(existing, expand)* | 118 | `List_of_chemical_elements` (table_col) | 118 | **hard cap = 118** |
| fruits *(existing, expand)* | ~80 | `List_of_culinary_fruits` (links/table) | 200 | |
| planets *(existing, expand)* | ~13 | none (curated only) | 13 | **hard cap**; planets + dwarf planets |

`hard_things` definition: things idiomatically or literally hard — hard
materials (diamond, tungsten, carbide, sapphire, …) plus notoriously difficult
games/feats (Dark Souls, Castlevania, Nurburgring, Sekiro, …). Curated only.

## 5. Security & Tooling

- **Proxy credentials**: removed from source. `main.py` reads
  `SCRAPER_PROXY_URL` from environment (loaded from a **gitignored `.env`**).
  Unset → scrape directly (no proxy). `.env.example` documents the variable;
  `.env` and `.python-version` added to `.gitignore` if not already ignored.
- **Dependencies** added to `pyproject.toml`: `requests`, `PySocks` (enables
  `socks5://` proxies), `beautifulsoup4`, `lxml`, `unidecode`. Run with
  `uv run python main.py`.
- Politeness preserved: existing per-request delay with jitter, descriptive
  User-Agent, `raise_for_status`, per-source try/except so one bad page never
  aborts the run.

## 6. Non-Goals (YAGNI)

- No live API integrations (BoardGameGeek, Leafly) — Wikipedia + seeds only.
- No incremental/caching layer — single full run regenerates the file.
- No changes to the Rust bot code; only `src/categories.json` (data) and
  Python/tooling files change.
- No transliteration of non-Latin scripts beyond what `unidecode` does by
  default; names that clean to `<3` chars are simply dropped.

## 7. Acceptance Criteria

1. `uv run python main.py` completes without an unhandled exception, even with
   `SCRAPER_PROXY_URL` unset and even if individual pages fail.
2. `src/categories.json` contains all existing 8 categories **expanded** plus
   ≥10 new categories, including `fictional_villainesses` and **no**
   `woman_murders`.
3. Every non-capped category meets its `min_target`; capped categories
   (`constellations`=88, `elements`=118, `planets`≤13, `spices`≈150) are at
   their natural maximum.
4. `cargo test` passes (all `src/data.rs` integrity tests green).
5. No credentials present in any committed file; `main.py` is committable.
6. Run summary prints per-category old/new/total counts and flags any category
   below `min_target`.

## 8. Verification Plan

- `uv run python main.py` (with and without proxy) → inspect printed summary.
- `python3 -c "import json; d=json.load(open('src/categories.json')); print({k:len(v) for k,v in sorted(d.items())})"`
- `cargo test` (exercises all integrity invariants in `src/data.rs`).
- `git grep -nE 'socks5://|proxy\.ziny\.io'` returns nothing in tracked files.
