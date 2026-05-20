#!/usr/bin/env python3
"""Seed-backed Wikipedia scraper for src/categories.json.

Loads curated seed lists already committed in src/categories.json, scrapes
Wikipedia on top of them, merges without ever dropping seeds, and rewrites
the file. Run: `uv run python main.py`. Proxy via SCRAPER_PROXY_URL env.

Pass `--only CATEGORY ...` to scope updates. Pass `--replace` (requires
`--only`) to discard existing entries for those categories and write just
the freshly-scraped names. By default `--replace` refuses to wipe a non-empty
category when the scrape returns zero entries; pass `--allow-empty` to
override.
"""
import argparse
import json
import os
import random
import re
import time
from pathlib import Path

import requests
from bs4 import BeautifulSoup, Tag
from unidecode import unidecode

# ---------------------------------------------------------------- cleaning ---

_ALLOWED = re.compile(r"[^A-Za-z0-9 -]")
_WS = re.compile(r"\s+")
_SEPARATORS = (" – ", " — ", " - ", " (", "(")

STOPWORDS = {
    "list", "lists", "index", "references", "see also", "external links",
    "notes", "isbn", "wikipedia", "category", "categories", "portal",
    "edit", "vte", "contents", "main page", "bibliography",
    "further reading", "sources", "gallery", "navigation",
}
STOPWORDS |= {chr(c) for c in range(ord("a"), ord("z") + 1)}

_JUNK_PREFIXES = (
    "list of ", "lists of ", "index of ", "outline of ", "timeline of ",
    "history of ", "glossary of ", "comparison of ", "bibliography of ",
    "table of ",
)
_JUNK_SUBSTRINGS = (" industry in ",)
_JUNK_EXCEPTIONS = frozenset({
    "history of the world",  # real Avalon Hill / Ragnar Brothers board game
})


def clean_name(raw: str | None) -> str | None:
    """Normalise a raw scraped string, or None if it is not a usable name."""
    if not raw:
        return None
    name = raw.strip()
    for sep in _SEPARATORS:
        if sep in name:
            name = name.split(sep, 1)[0]
            break
    name = unidecode(name)
    name = _ALLOWED.sub("", name)
    name = _WS.sub(" ", name).strip()
    if len(name) < 3 or len(name) > 32:
        return None
    if name.lower() in STOPWORDS:
        return None
    low = name.lower()
    if low not in _JUNK_EXCEPTIONS and (
        low.startswith(_JUNK_PREFIXES)
        or any(s in low for s in _JUNK_SUBSTRINGS)
    ):
        return None
    return name


def dedupe_keep_first(names: list[str]) -> list[str]:
    """Case-insensitive dedupe preserving first-seen casing and order."""
    seen: set[str] = set()
    out: list[str] = []
    for n in names:
        k = n.lower()
        if k not in seen:
            seen.add(k)
            out.append(n)
    return out


# -------------------------------------------------------------- extraction ---

_EXCLUDE = [
    ".navbox", ".reflist", "ol.references", "#toc", ".toc",
    ".mw-editsection", "table.metadata", ".navbar", "style", "script",
    ".thumb", ".gallery", ".sistersitebox", ".hatnote",
]
_STOP_SECTIONS = {
    "see also", "references", "external links", "notes",
    "further reading", "bibliography", "sources",
}
_BAD_HREF = ("File:", "Category:", "Help:", "Wikipedia:",
             "Template:", "Portal:", "Special:", "Talk:")


def _content(html: str) -> Tag | BeautifulSoup:
    """Return the article body with nav/ref/see-also chrome removed."""
    soup = BeautifulSoup(html, "lxml")
    # `mw-parser-output` is the article body wrapper on essentially every
    # Wikipedia content page; `mw-content-text` is the broader outer wrapper
    # for the rare page that lacks `mw-parser-output`. Fall back to the whole
    # document only as a last resort.
    body = (
        soup.select_one("div.mw-parser-output")
        or soup.select_one("div.mw-content-text")
        or soup
    )
    for sel in _EXCLUDE:
        for el in body.select(sel):
            el.decompose()
    for h in body.find_all("h2"):
        if h.get_text(strip=True).lower() in _STOP_SECTIONS:
            for sib in list(h.next_siblings):
                if getattr(sib, "decompose", None):
                    sib.decompose()
            h.decompose()
            break
    return body


def extract_links(html: str, options: dict | None = None) -> list[str]:
    """Article-link titles in the body, excluding namespaced links."""
    body = _content(html)
    out: list[str] = []
    for a in body.select("a[href^='/wiki/']"):
        slug = a["href"][len("/wiki/"):]
        if any(slug.startswith(p) for p in _BAD_HREF):
            continue
        text = a.get_text(" ", strip=True)
        if text:
            out.append(text)
    return out


def extract_bullets(html: str, options: dict | None = None) -> list[str]:
    """Raw `<li>` text from body bullet lists (clean_name splits later).

    `options` is accepted for interface symmetry but unused. Nested
    sub-list items also surface individually; bloated parent strings are
    dropped later by clean_name's length gate.
    """
    body = _content(html)
    return [li.get_text(" ", strip=True) for li in body.select("ul > li")]


def extract_table_col(html: str, options: dict | None = None) -> list[str]:
    """Column `options['col']` (default 0) of each wikitable data row.

    Source-specific overrides via `options`:
      * `"table_selector"`: CSS selector for the data table when it isn't a
        `.wikitable` (default `"table.wikitable"`).
      * `"swap_on_comma"`: if true, rewrite cell text containing `", "` from
        `"Last, First"` to `"First Last"` BEFORE `clean_name` strips the
        comma. Use for Wikipedia tables sorted surname-first
        (e.g. `List_of_serial_killers_in_the_United_States`).
      * `"scope": "soup"`: bypass `_content`'s `mw-parser-output` scoping and
        operate on the whole document (still applies `_EXCLUDE` to strip
        nav/ref chrome). Needed for the rare page whose data table sits
        outside `mw-parser-output`.
    """
    opts = options or {}
    col = opts.get("col", 0)
    table_sel = opts.get("table_selector", "table.wikitable")
    swap = bool(opts.get("swap_on_comma"))
    if opts.get("scope") == "soup":
        body = BeautifulSoup(html, "lxml")
        for sel in _EXCLUDE:
            for el in body.select(sel):
                el.decompose()
    else:
        body = _content(html)
    out: list[str] = []
    for table in body.select(table_sel):
        for tr in table.select("tr"):
            cells = tr.find_all(["td", "th"], recursive=False)
            if not cells or all(c.name == "th" for c in cells):
                continue  # skip the header row (and empty rows)
            if len(cells) <= col:
                continue
            cell = cells[col]
            a = cell.find("a")
            text = (a.get_text(" ", strip=True) if a
                    else cell.get_text(" ", strip=True))
            if swap and ", " in text:
                last, first = text.split(", ", 1)
                text = f"{first} {last}"
            out.append(text)
    return out


# ----------------------------------------------------------------- fetch ---

USER_AGENT = "ChaoticNickBot/1.0 (Discord novelty bot category builder)"


def get_session() -> requests.Session:
    s = requests.Session()
    proxy = os.environ.get("SCRAPER_PROXY_URL", "").strip()
    if proxy:
        s.proxies = {"http": proxy, "https": proxy}
    return s


def fetch(session: requests.Session, url: str, delay: float = 1.0) -> str | None:
    try:
        r = session.get(url, headers={"User-Agent": USER_AGENT}, timeout=20)
        r.raise_for_status()
        time.sleep(delay + random.uniform(0.5, 1.5))
        print(f"✅ fetched {url}")
        return r.text
    except Exception as e:  # noqa: BLE001 - one bad page must not abort the run
        print(f"❌ fetch failed {url}: {e}")
        return None


# ----------------------------------------------------------- orchestration ---

CATEGORIES_PATH = Path("src/categories.json")
W = "https://en.wikipedia.org/wiki/"

STRATEGIES = {
    "links": extract_links,
    "bullets": extract_bullets,  # reserved: no SOURCES entry uses it yet
    "table_col": extract_table_col,
}

SOURCES: dict[str, list[tuple]] = {
    "serial_killers": [(W + "List_of_serial_killers_in_the_United_States",
                        "table_col",
                        {"col": 0, "table_selector": "table",
                         "swap_on_comma": True, "scope": "soup"})],
    "board_games": [(W + "List_of_board_games", "links", {})],
    "cocktails": [(W + "List_of_cocktails", "links", {}),
                  (W + "IBA_official_cocktail", "links", {})],
    "mythical_creatures": [
        (W + f"List_of_legendary_creatures_({c})", "links", {})
        for c in "ABCDEFGHIJKLMNOPQRSTUVWXYZ"],
    "superheroes": [(W + "List_of_superheroes", "links", {}),
                    (W + "List_of_DC_Comics_characters", "links", {}),
                    (W + "List_of_Marvel_Comics_characters", "links", {})],
    "cars": [(W + "List_of_car_brands", "links", {}),
             (W + "List_of_automobiles", "links", {})],
    "scientists": [(W + "List_of_physicists", "links", {}),
                   (W + "List_of_American_mathematicians", "links", {}),
                   (W + "List_of_Greek_mathematicians", "links", {})],
    "strains_weed": [(W + "List_of_Cannabis_strains", "links", {})],
    "fictional_villainesses": [
        (W + "List_of_female_supervillains", "links", {})],
    "hard_things": [],
    "constellations": [
        (W + "IAU_designated_constellations", "table_col", {"col": 0})],
    "spices": [(W + "List_of_culinary_herbs_and_spices", "links", {}),
               (W + "List_of_herbs", "links", {}),
               (W + "List_of_spice_mixes", "links", {})],
    "amusement_parks": [
        (W + "List_of_amusement_parks_in_the_Americas", "links", {}),
        (W + "List_of_amusement_parks_in_Europe", "links", {})],
    "chemical_compounds": [(W + "List_of_compounds", "links", {})],
    "colors": [(W + "List_of_colors_(compact)", "links", {})],
    "dinosaurs": [(W + "List_of_dinosaur_genera", "links", {})],
    "elements": [
        (W + "List_of_chemical_elements", "table_col", {"col": 2})],
    "fruits": [(W + "List_of_culinary_fruits", "links", {})],
    "planets": [],
}

MIN_TARGET = {
    "board_games": 300, "cocktails": 300, "mythical_creatures": 300,
    "superheroes": 300, "cars": 300, "scientists": 300, "strains_weed": 200,
    "fictional_villainesses": 150, "hard_things": 100, "constellations": 88,
    "spices": 300, "amusement_parks": 200, "chemical_compounds": 150,
    "colors": 300, "dinosaurs": 300, "elements": 118, "fruits": 200,
    "planets": 13, "serial_killers": 100,
}


def scrape_category(session: requests.Session, name: str) -> list[str]:
    raw: list[str] = []
    for (url, strat, opts) in SOURCES.get(name, []):
        html = fetch(session, url)
        if html is None:
            print(f"❌ skipping {url} due to html being None")
            continue
        try:
            raw.extend(STRATEGIES[strat](html, opts))
        except Exception as e:  # noqa: BLE001
            print(f"❌ extract failed {url} ({strat}): {e}")
    cleaned = [c for c in (clean_name(r) for r in raw) if c]
    return dedupe_keep_first(cleaned)


def main(
    only: list[str] | None = None,
    replace: bool = False,
    allow_empty: bool = False,
) -> None:
    if replace and not only:
        print("❌ --replace requires --only to specify which categories to "
              "wipe; refusing to nuke every category at once.")
        return

    data = json.loads(CATEGORIES_PATH.read_text(encoding="utf-8"))

    all_cats = sorted(set(data) | set(SOURCES))
    if only:
        requested = set(only)
        unknown = sorted(requested - set(all_cats))
        if unknown:
            print(f"⚠️  Unknown categories ignored: {', '.join(unknown)}")
        cats = [c for c in all_cats if c in requested]
        if not cats:
            print("❌ No known categories selected; nothing to do.")
            return
        mode = "Replacing" if replace else "Updating"
        print(f"▶️  {mode} only: {', '.join(cats)}")
    else:
        cats = all_cats

    session = get_session()
    report: dict[str, tuple[int, int, int]] = {}
    refused: list[tuple[str, int]] = []

    for cat in cats:
        existing = data.get(cat, [])
        if replace:
            seed: list[str] = []
        else:
            seed = dedupe_keep_first(
                [s for s in (clean_name(x) for x in existing) if s])
        scraped = scrape_category(session, cat)
        # Defensive re-clean: clean_name is idempotent, so this is a no-op for
        # the real path (scrape_category already cleans) but guarantees the
        # written file is always valid regardless of how names arrived.
        scraped = [c for c in (clean_name(x) for x in scraped) if c]
        if replace and not scraped and existing and not allow_empty:
            refused.append((cat, len(existing)))
            print(f"❌ {cat}: scrape returned 0 entries; refusing to wipe "
                  f"{len(existing)} existing (pass --allow-empty to override)")
            continue
        merged = dedupe_keep_first(seed + scraped)
        data[cat] = merged
        report[cat] = (len(seed), len(merged) - len(seed), len(merged))

    CATEGORIES_PATH.write_text(
        json.dumps(data, indent=2, ensure_ascii=False, sort_keys=True),
        encoding="utf-8")

    print("\n✅ src/categories.json updated")
    for cat in sorted(report):
        seed_n, new_n, total = report[cat]
        target = MIN_TARGET.get(cat, 0)
        flag = f"  ⚠️ below target {target}" if total < target else ""
        print(f"  {cat:24} seed {seed_n:4}  +{new_n:4}  = {total:4}{flag}")

    if refused:
        print(f"\n⚠️  Refused {len(refused)} replacement(s) "
              "(empty scrape, --allow-empty not set):")
        for cat, n in refused:
            print(f"  {cat:24} kept {n} existing")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Populate src/categories.json from curated seeds + "
        "Wikipedia scraping.")
    parser.add_argument(
        "--only",
        nargs="+",
        metavar="CATEGORY",
        help="Only update these categories (space-separated); all other "
        "categories in src/categories.json are left unchanged.")
    parser.add_argument(
        "--replace",
        action="store_true",
        help="For each --only category, discard existing entries and write "
        "just the freshly-scraped names. Requires --only. Refuses to wipe a "
        "non-empty category when the scrape returns zero unless "
        "--allow-empty is also passed.")
    parser.add_argument(
        "--allow-empty",
        action="store_true",
        help="With --replace, permit overwriting a non-empty category with "
        "an empty scrape result.")
    args = parser.parse_args()
    main(only=args.only, replace=args.replace, allow_empty=args.allow_empty)
