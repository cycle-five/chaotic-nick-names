# Resilient Category Scraper Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild `main.py` into a seed-backed Wikipedia scraper that populates `src/categories.json` with 18 categories (8 expanded + 10 new) at 300–500 names each where feasible, and can be run safely tonight.

**Architecture:** Single-file Python tool. Pure helpers (`clean_name`, `dedupe_keep_first`, three extraction strategies) are unit-tested against inline HTML fixtures. A `SOURCES` config table maps each category to Wikipedia URLs + an extraction strategy. `main()` loads the committed curated seed lists from `src/categories.json`, scrapes on top, merges (never dropping seeds), and rewrites the JSON. Proxy credentials come from `SCRAPER_PROXY_URL` env, never source.

**Tech Stack:** Python 3.13, `uv`, `requests` + `PySocks`, `beautifulsoup4` + `lxml`, `unidecode`, `pytest`. Data is consumed by the existing Rust bot via `src/data.rs` (`include_str!`), whose tests in `src/data.rs` are the integrity gate.

**Spec:** `docs/superpowers/specs/2026-05-17-resilient-category-scraper-design.md`

---

## File Structure

- `pyproject.toml` — add runtime + dev deps, pytest config (modify)
- `.gitignore` — add Python ignores (modify)
- `.env.example` — document `SCRAPER_PROXY_URL` (create)
- `main.py` — full rewrite: imports → pure helpers → extraction → fetch → SOURCES → `scrape_category` → `main` (modify/overwrite)
- `tests/test_clean.py` — `clean_name` / `dedupe_keep_first` (create)
- `tests/test_extract.py` — extraction strategies vs inline HTML (create)
- `tests/test_scrape.py` — `get_session`, `scrape_category`, `main` merge (create)
- `src/categories.json` — curated seed lists for all 18 categories (modify/overwrite)

---

## Task 1: Project tooling & dependencies

**Files:**
- Modify: `pyproject.toml`
- Modify: `.gitignore`
- Create: `.env.example`

- [ ] **Step 1: Replace `pyproject.toml`**

```toml
[project]
name = "chaotic-nick-names"
version = "0.1.0"
description = "Category data scraper for the chaotic-nick-names Discord bot"
readme = "README.md"
requires-python = ">=3.13"
dependencies = [
    "requests>=2.31",
    "PySocks>=1.7",
    "beautifulsoup4>=4.12",
    "lxml>=5.0",
    "unidecode>=1.3",
]

[dependency-groups]
dev = ["pytest>=8.0"]

[tool.pytest.ini_options]
pythonpath = ["."]
testpaths = ["tests"]
```

- [ ] **Step 2: Append Python ignores to `.gitignore`**

Append these lines to the existing `.gitignore` (keep current contents):

```
.python-version
__pycache__/
*.pyc
.pytest_cache/
```

- [ ] **Step 3: Create `.env.example`**

```
# Optional proxy for the category scraper (main.py). Unset = direct connection.
# Supports http(s):// and socks5:// (PySocks installed for socks5).
# Example: socks5://user:pass@host:port
SCRAPER_PROXY_URL=
```

- [ ] **Step 4: Verify deps install and import**

Run: `uv run python -c "import requests, bs4, lxml, unidecode; print('ok')"`
Expected: prints `ok` (uv resolves/installs the new deps).

- [ ] **Step 5: Commit**

```bash
git add pyproject.toml .gitignore .env.example
git commit -m "build: add scraper deps, pytest config, env-based proxy doc

Co-Authored-By: Claude & Lothrop (cycle.five@proton.me)"
```

---

## Task 2: Cleaning pipeline (`clean_name`, `dedupe_keep_first`)

**Files:**
- Modify: `main.py` (full overwrite with the skeleton below)
- Create: `tests/test_clean.py`

- [ ] **Step 1: Write the failing test — `tests/test_clean.py`**

```python
from main import clean_name, dedupe_keep_first


def test_transliterates_accents():
    assert clean_name("André") == "Andre"
    assert clean_name("Lovász") == "Lovasz"
    assert clean_name("Café Bustelo") == "Cafe Bustelo"


def test_trims_and_collapses_whitespace():
    assert clean_name("  T-Rex  ") == "T-Rex"
    assert clean_name("Blue   Dream") == "Blue Dream"


def test_cuts_at_description_separators():
    assert clean_name("Margarita (cocktail)") == "Margarita"
    assert clean_name("Gin – a juniper spirit") == "Gin"
    assert clean_name("Sazerac - New Orleans") == "Sazerac"


def test_length_bounds():
    assert clean_name("ab") is None          # < 3
    assert clean_name("x" * 33) is None      # > 32
    assert clean_name("x" * 32) == "x" * 32  # exactly 32 ok


def test_rejects_stopwords_and_section_letters():
    assert clean_name("References") is None
    assert clean_name("See also") is None
    assert clean_name("A") is None
    assert clean_name("Z") is None


def test_strips_disallowed_characters():
    assert clean_name("AC/DC!!") == "ACDC"
    assert clean_name("???") is None


def test_dedupe_keep_first_case_insensitive():
    assert dedupe_keep_first(["Gin", "gin", "GIN", "Rum"]) == ["Gin", "Rum"]
    assert dedupe_keep_first([]) == []
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `uv run pytest tests/test_clean.py -q`
Expected: FAIL — `ModuleNotFoundError`/`ImportError` (no `clean_name` in `main.py`).

- [ ] **Step 3: Overwrite `main.py` with the skeleton + pipeline**

```python
#!/usr/bin/env python3
"""Seed-backed Wikipedia scraper for src/categories.json.

Loads curated seed lists already committed in src/categories.json, scrapes
Wikipedia on top of them, merges without ever dropping seeds, and rewrites
the file. Run: `uv run python main.py`. Proxy via SCRAPER_PROXY_URL env.
"""
import json
import os
import random
import re
import time
from pathlib import Path

import requests
from bs4 import BeautifulSoup
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
STOPWORDS |= {chr(c) for c in range(ord("A"), ord("Z") + 1)}


def clean_name(raw: str | None) -> str | None:
    """Normalise a raw scraped string, or None if it is not a usable name."""
    if not raw:
        return None
    name = unidecode(raw).strip()
    for sep in _SEPARATORS:
        if sep in name:
            name = name.split(sep, 1)[0]
            break
    name = _ALLOWED.sub("", name)
    name = _WS.sub(" ", name).strip()
    if len(name) < 3 or len(name) > 32:
        return None
    if name.lower() in STOPWORDS:
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
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `uv run pytest tests/test_clean.py -q`
Expected: PASS (7 passed).

- [ ] **Step 5: Commit**

```bash
git add main.py tests/test_clean.py
git commit -m "feat(scraper): cleaning pipeline (clean_name, dedupe_keep_first)

Co-Authored-By: Claude & Lothrop (cycle.five@proton.me)"
```

---

## Task 3: Content scoping + `extract_links`

**Files:**
- Modify: `main.py` (append)
- Create: `tests/test_extract.py`

- [ ] **Step 1: Write the failing test — `tests/test_extract.py`**

```python
from main import extract_links

LINKS_HTML = """
<html><body>
<div class="mw-parser-output">
  <ul>
    <li><a href="/wiki/Catan">Catan</a></li>
    <li><a href="/wiki/Carcassonne_(board_game)">Carcassonne</a></li>
    <li><a href="/wiki/Category:Board_games">Board games category</a></li>
    <li><a href="/wiki/File:Dice.png">a file</a></li>
  </ul>
  <div class="navbox"><a href="/wiki/NavLink">NavLink</a></div>
  <div class="reflist"><a href="/wiki/RefLink">RefLink</a></div>
  <h2>See also</h2>
  <ul><li><a href="/wiki/SeeAlsoGame">SeeAlsoGame</a></li></ul>
</div>
</body></html>
"""


def test_extract_links_keeps_real_article_links():
    got = extract_links(LINKS_HTML, {})
    assert "Catan" in got
    assert "Carcassonne" in got


def test_extract_links_drops_namespaced_nav_ref_and_see_also():
    got = extract_links(LINKS_HTML, {})
    assert "Board games category" not in got
    assert "a file" not in got
    assert "NavLink" not in got
    assert "RefLink" not in got
    assert "SeeAlsoGame" not in got
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `uv run pytest tests/test_extract.py -q`
Expected: FAIL — `ImportError` (no `extract_links`).

- [ ] **Step 3: Append content scoping + `extract_links` to `main.py`**

```python
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


def _content(html: str) -> BeautifulSoup:
    """Return the article body with nav/ref/see-also chrome removed."""
    soup = BeautifulSoup(html, "lxml")
    body = soup.select_one("div.mw-parser-output") or soup
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
        out.append(a.get_text(" ", strip=True))
    return out
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `uv run pytest tests/test_extract.py -q`
Expected: PASS (2 passed).

- [ ] **Step 5: Commit**

```bash
git add main.py tests/test_extract.py
git commit -m "feat(scraper): content scoping + extract_links strategy

Co-Authored-By: Claude & Lothrop (cycle.five@proton.me)"
```

---

## Task 4: `extract_bullets` + `extract_table_col`

**Files:**
- Modify: `main.py` (append)
- Modify: `tests/test_extract.py` (append)

- [ ] **Step 1: Append failing tests to `tests/test_extract.py`**

```python
from main import extract_bullets, extract_table_col

BULLETS_HTML = """
<html><body><div class="mw-parser-output"><ul>
  <li>Gin – a juniper spirit</li>
  <li>Margarita (cocktail)</li>
  <li>Negroni</li>
</ul></div></body></html>
"""

TABLE_HTML = """
<html><body><div class="mw-parser-output">
<table class="wikitable">
  <tr><th>Idx</th><th>Name</th></tr>
  <tr><td>1</td><td><a href="/wiki/Andromeda">Andromeda</a></td></tr>
  <tr><td>2</td><td><a href="/wiki/Aquarius">Aquarius</a></td></tr>
  <tr><td>3</td><td>Lyra</td></tr>
</table>
</div></body></html>
"""


def test_extract_bullets_returns_raw_li_text():
    got = extract_bullets(BULLETS_HTML, {})
    assert "Gin – a juniper spirit" in got
    assert "Negroni" in got


def test_extract_table_col_picks_chosen_column():
    got = extract_table_col(TABLE_HTML, {"col": 1})
    assert got == ["Andromeda", "Aquarius", "Lyra"]


def test_extract_table_col_defaults_to_first_column():
    got = extract_table_col(TABLE_HTML, {})
    assert got == ["1", "2", "3"]
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `uv run pytest tests/test_extract.py -q`
Expected: FAIL — `ImportError` (no `extract_bullets` / `extract_table_col`).

- [ ] **Step 3: Append both strategies to `main.py`**

```python
def extract_bullets(html: str, options: dict | None = None) -> list[str]:
    """Raw `<li>` text from body bullet lists (clean_name splits later)."""
    body = _content(html)
    return [li.get_text(" ", strip=True) for li in body.select("ul > li")]


def extract_table_col(html: str, options: dict | None = None) -> list[str]:
    """Column `options['col']` (default 0) of each wikitable data row."""
    col = (options or {}).get("col", 0)
    body = _content(html)
    out: list[str] = []
    for table in body.select("table.wikitable"):
        for tr in table.select("tr"):
            cells = tr.find_all(["td", "th"], recursive=False)
            if not cells or all(c.name == "th" for c in cells):
                continue  # skip the header row (and empty rows)
            if len(cells) <= col:
                continue
            cell = cells[col]
            a = cell.find("a")
            out.append(a.get_text(" ", strip=True) if a
                       else cell.get_text(" ", strip=True))
    return out
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `uv run pytest tests/test_extract.py -q`
Expected: PASS (6 passed total in the file — 3 from Task 3 incl. the
real-wiki h2 test, plus the 3 added here).

- [ ] **Step 5: Commit**

```bash
git add main.py tests/test_extract.py
git commit -m "feat(scraper): extract_bullets + extract_table_col strategies

Co-Authored-By: Claude & Lothrop (cycle.five@proton.me)"
```

---

## Task 5: HTTP session (env proxy) + `fetch`

**Files:**
- Modify: `main.py` (append)
- Create: `tests/test_scrape.py`

- [ ] **Step 1: Write the failing test — `tests/test_scrape.py`**

```python
from main import get_session


def test_get_session_no_proxy(monkeypatch):
    monkeypatch.delenv("SCRAPER_PROXY_URL", raising=False)
    s = get_session()
    assert s.proxies == {}


def test_get_session_with_proxy(monkeypatch):
    monkeypatch.setenv("SCRAPER_PROXY_URL", "socks5://u:p@host:1080")
    s = get_session()
    assert s.proxies == {
        "http": "socks5://u:p@host:1080",
        "https": "socks5://u:p@host:1080",
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `uv run pytest tests/test_scrape.py -q`
Expected: FAIL — `ImportError` (no `get_session`).

- [ ] **Step 3: Append session + fetch to `main.py`**

```python
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
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `uv run pytest tests/test_scrape.py -q`
Expected: PASS (2 passed).

- [ ] **Step 5: Commit**

```bash
git add main.py tests/test_scrape.py
git commit -m "feat(scraper): env-driven session + resilient fetch

Co-Authored-By: Claude & Lothrop (cycle.five@proton.me)"
```

---

## Task 6: `SOURCES` config + `scrape_category` + `main` merge

**Files:**
- Modify: `main.py` (append)
- Modify: `tests/test_scrape.py` (append)

- [ ] **Step 1: Append failing tests to `tests/test_scrape.py`**

```python
import json
import main as m


def test_sources_has_new_and_no_woman_murders():
    assert "fictional_villainesses" in m.SOURCES
    assert "woman_murders" not in m.SOURCES
    for c in ("board_games", "cocktails", "mythical_creatures",
              "superheroes", "cars", "strains_weed", "hard_things",
              "constellations", "spices", "scientists"):
        assert c in m.SOURCES


def test_main_merges_seeds_with_scrape(tmp_path, monkeypatch):
    cats = tmp_path / "categories.json"
    cats.write_text(json.dumps({"cocktails": ["Negroni", "Martini"]}),
                    encoding="utf-8")
    monkeypatch.setattr(m, "CATEGORIES_PATH", cats)
    monkeypatch.setattr(m, "get_session", lambda: object())
    # scrape returns a new name, a dup of a seed, and junk
    monkeypatch.setattr(
        m, "scrape_category",
        lambda session, name: ["Sazerac", "negroni", "ab"]
        if name == "cocktails" else [])

    m.main()

    data = json.loads(cats.read_text(encoding="utf-8"))
    assert data["cocktails"][:2] == ["Negroni", "Martini"]   # seeds preserved
    assert "Sazerac" in data["cocktails"]                     # new added
    assert data["cocktails"].count("Negroni") == 1            # dup dropped
    assert "ab" not in data["cocktails"]                       # junk cleaned
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `uv run pytest tests/test_scrape.py -q`
Expected: FAIL — `AttributeError`/`ImportError` (`SOURCES`, `scrape_category`, `main`, `CATEGORIES_PATH` not defined).

- [ ] **Step 3: Append config + orchestration + main to `main.py`**

```python
# ----------------------------------------------------------- orchestration ---

CATEGORIES_PATH = Path("src/categories.json")
W = "https://en.wikipedia.org/wiki/"

STRATEGIES = {
    "links": extract_links,
    "bullets": extract_bullets,
    "table_col": extract_table_col,
}

SOURCES: dict[str, list[tuple]] = {
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
                   (W + "List_of_chemists", "links", {}),
                   (W + "List_of_biologists", "links", {}),
                   (W + "List_of_astronomers", "links", {}),
                   (W + "List_of_mathematicians", "links", {})],
    "strains_weed": [(W + "List_of_Cannabis_strains", "links", {})],
    "fictional_villainesses": [
        (W + "List_of_female_supervillains", "links", {})],
    "hard_things": [],
    "constellations": [
        (W + "IAU_designated_constellations", "table_col", {"col": 1})],
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
    "planets": 13,
}


def scrape_category(session, name: str) -> list[str]:
    raw: list[str] = []
    for (url, strat, opts) in SOURCES.get(name, []):
        html = fetch(session, url)
        if html is None:
            continue
        try:
            raw.extend(STRATEGIES[strat](html, opts))
        except Exception as e:  # noqa: BLE001
            print(f"❌ extract failed {url} ({strat}): {e}")
    cleaned = [c for c in (clean_name(r) for r in raw) if c]
    return dedupe_keep_first(cleaned)


def main() -> None:
    data = json.loads(CATEGORIES_PATH.read_text(encoding="utf-8"))
    session = get_session()
    report: dict[str, tuple[int, int, int]] = {}

    for cat in sorted(set(data) | set(SOURCES)):
        seed = dedupe_keep_first(
            [s for s in (clean_name(x) for x in data.get(cat, [])) if s])
        scraped = scrape_category(session, cat)
        # Defensive re-clean: clean_name is idempotent, so this is a no-op for
        # the real path (scrape_category already cleans) but guarantees the
        # written file is always valid regardless of how names arrived.
        scraped = [c for c in (clean_name(x) for x in scraped) if c]
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


if __name__ == "__main__":
    main()
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `uv run pytest -q`
Expected: PASS — entire suite green (test_clean + test_extract + test_scrape).

- [ ] **Step 5: Commit**

```bash
git add main.py tests/test_scrape.py
git commit -m "feat(scraper): SOURCES config, scrape_category, seed-merge main

Co-Authored-By: Claude & Lothrop (cycle.five@proton.me)"
```

---

## Task 7: Curated seed lists → `src/categories.json`

This task replaces `src/categories.json` so every category ships a working
baseline. **No live scraping yet** — this is the reliability floor that
guarantees the bot works tonight even if every scrape fails.

**Files:**
- Modify: `src/categories.json` (overwrite)

- [ ] **Step 1: Write `src/categories.json` with all 18 seeded categories**

Produce a single JSON object, keys sorted, of shape `{category: [names]}`.
Include **exactly these 18 keys** and **no others** (no `woman_murders`):

`amusement_parks`, `board_games`, `cars`, `chemical_compounds`, `cocktails`,
`colors`, `constellations`, `dinosaurs`, `elements`, `fictional_villainesses`,
`fruits`, `hard_things`, `mythical_creatures`, `planets`, `scientists`,
`spices`, `strains_weed`, `superheroes`.

Minimum seed count per category (more is fine; capped categories must be
**complete**, not merely at the minimum):

| Category | Min seed | Hard cap? | Flavor examples (match this set) |
|---|---|---|---|
| board_games | 120 | no | Catan, Carcassonne, Pandemic, Azul, Wingspan, Scythe, Gloomhaven, Risk, Clue, Codenames |
| cocktails | 120 | no | Negroni, Sazerac, Margarita, Daiquiri, Manhattan, Mojito, Aviation, Paloma, Boulevardier, Sidecar |
| mythical_creatures | 120 | no | Phoenix, Kraken, Basilisk, Wendigo, Kitsune, Selkie, Manticore, Chimera, Banshee, Griffin |
| superheroes | 120 | no | Spider-Man, Storm, Nightcrawler, Daredevil, Vixen, Cyborg, Rorschach, Hellboy, Spawn, Blade |
| cars | 120 | no | Mustang, Corvette, Countach, Supra, Delorean, Impreza, Charger, Miata, Testarossa, Skyline |
| scientists | 120 | no | Einstein, Curie, Feynman, Turing, Noether, Ramanujan, Dirac, Pauling, Hopper, Faraday |
| strains_weed | 120 | no | Blue Dream, Sour Diesel, OG Kush, Gelato, Northern Lights, Granddaddy Purple, Pineapple Express, Zkittlez, GSC, Trainwreck |
| fictional_villainesses | 120 | no | Maleficent, Cruella, Ursula, Bellatrix, Cersei, Mystique, Poison Ivy, Harley Quinn, Catwoman, Nurse Ratched |
| hard_things | 100 | soft (~100) | Diamond, Tungsten, Tungsten Carbide, Sapphire, Dark Souls, Sekiro, Castlevania, Contra, Nurburgring, Calculus |
| constellations | 88 | **yes = 88** | Andromeda, Orion, Cassiopeia, Lyra, Draco, Cygnus, Perseus, Hydra, Carina, Vela (all 88 IAU) |
| spices | 120 | no (target 300) | Cumin, Cardamom, Sumac, Saffron, Fenugreek, Garam Masala, Za'atar, Herbes de Provence, Ras el Hanout, Shichimi Togarashi |
| amusement_parks | 80 | no | Disneyland, Cedar Point, Tokyo DisneySea, Europa Park, Alton Towers, Knotts Berry Farm, Tivoli Gardens, PortAventura, Efteling, Liseberg |
| chemical_compounds | 80 | no | Caffeine, Aspirin, Serotonin, Glucose, Ethanol, Adrenaline, Methane, Ammonia, Histamine, Capsaicin |
| colors | 80 | no | Crimson, Vermilion, Cerulean, Chartreuse, Amaranth, Periwinkle, Sienna, Ultramarine, Fuchsia, Mauve |
| dinosaurs | 80 | no | Tyrannosaurus, Velociraptor, Triceratops, Brachiosaurus, Allosaurus, Ankylosaurus, Spinosaurus, Stegosaurus, Diplodocus, Iguanodon |
| elements | 118 | **yes = 118** | Hydrogen, Helium, Carbon, Oxygen, Iron, Gold, Uranium, Neon, Sodium, Tungsten (full periodic table) |
| fruits | 80 | no | Mango, Papaya, Persimmon, Lychee, Rambutan, Durian, Soursop, Tamarind, Quince, Loquat |
| planets | 13 | **yes = 13** | Mercury, Venus, Earth, Mars, Jupiter, Saturn, Uranus, Neptune, Pluto, Ceres, Eris, Makemake, Haumea |

Every name MUST already satisfy the integrity rules (they are not re-cleaned
on commit, only at scrape time): ASCII `^[A-Za-z0-9 -]+$`, length 3–32, no
duplicates within a category, no leading/trailing whitespace. (E.g. write
`Spider-Man` not `Spider‑Man`; `Za'atar` → `Zaatar`; `GSC` is fine.)

- [ ] **Step 2: Verify integrity with a Python guard**

Run:
```bash
uv run python -c "
import json, re
d = json.load(open('src/categories.json'))
need = {'amusement_parks':80,'board_games':120,'cars':120,'chemical_compounds':80,'cocktails':120,'colors':80,'constellations':88,'dinosaurs':80,'elements':118,'fictional_villainesses':120,'fruits':80,'hard_things':100,'mythical_creatures':120,'planets':13,'scientists':120,'spices':120,'strains_weed':120,'superheroes':120}
assert set(d) == set(need), set(d) ^ set(need)
pat = re.compile(r'^[A-Za-z0-9 -]+$')
for k, v in d.items():
    assert len(v) >= need[k], (k, len(v), need[k])
    assert len(v) == len({x.lower() for x in v}), ('dup in', k)
    for x in v:
        assert pat.match(x) and 3 <= len(x) <= 32 and x == x.strip(), (k, repr(x))
print('seed integrity OK', {k: len(v) for k in sorted(d) for v in [d[k]]})
"
```
Expected: prints `seed integrity OK {...}` with every count ≥ its minimum.

- [ ] **Step 3: Verify the Rust integrity tests pass on the seed data**

Run: `cargo test --quiet 2>&1 | tail -20`
Expected: all tests pass (the `src/data.rs` invariants: ≤32 chars, no dupes,
no edge whitespace, valid JSON, known categories present).

- [ ] **Step 4: Commit**

```bash
git add src/categories.json
git commit -m "feat(data): curated seed lists for all 18 categories

Replaces woman_murders with fictional_villainesses; expands the original 8.
Guaranteed working baseline independent of scraping.

Co-Authored-By: Claude & Lothrop (cycle.five@proton.me)"
```

---

## Task 8: Live end-to-end run & verification

**Files:** none modified (run + verify); commit the regenerated data.

- [ ] **Step 1: Sanity-check `SOURCES` URLs that use non-`links` strategies**

For `constellations` (`IAU_designated_constellations`, `table_col col=1`) and
`elements` (`List_of_chemical_elements`, `table_col col=2`), fetch each page
once and confirm the chosen column is the name column:

Run:
```bash
uv run python -c "
import main
s = main.get_session()
for name in ('constellations','elements'):
    print('===', name)
    print(main.scrape_category(s, name)[:8])
"
```
Expected: constellation/element **names** (not numbers/symbols) in the first 8.
If they are wrong, adjust the `col` index in `SOURCES` for that category in
`main.py`, re-run this step, then `git commit -am "fix(scraper): correct
table column for <cat>" ` with the required Co-Authored-By trailer.

- [ ] **Step 2: Full run without a proxy (direct connection)**

Run: `unset SCRAPER_PROXY_URL; uv run python main.py`
Expected: a per-category summary; the run completes with no traceback even if
some pages fail. Categories at/over `MIN_TARGET` print no `⚠️`.

- [ ] **Step 3 (optional): Full run via your proxy**

Set the proxy in `.env` or the shell, then:
Run: `set -a; source .env; set +a; uv run python main.py` (or
`SCRAPER_PROXY_URL=... uv run python main.py`)
Expected: same, larger counts where scraping succeeds. Skip if direct worked.

- [ ] **Step 4: Verify final data + integrity + no credential leak**

Run:
```bash
uv run python -c "import json; d=json.load(open('src/categories.json')); print({k:len(v) for k,v in sorted(d.items())})"
cargo test --quiet 2>&1 | tail -5
git grep -nE 'socks5://[^ "]*@|proxy\.ziny\.io' -- . ':!docs/**' || echo "NO CREDS IN TRACKED SOURCE"
```
Expected: counts meet targets (capped categories at their cap); `cargo test`
green; the grep prints `NO CREDS IN TRACKED SOURCE` (matches any
credentialed proxy URL without hardcoding the secret in this plan).

- [ ] **Step 5: Commit the regenerated data**

```bash
git add src/categories.json main.py
git commit -m "feat(data): populate categories via live Wikipedia scrape

Co-Authored-By: Claude & Lothrop (cycle.five@proton.me)"
```

---

## Self-Review Notes

- **Spec coverage:** §2 constraints → Task 7 guard + Task 2 cleaning; §3.1 SOURCES → Task 6; §3.2 strategies → Tasks 3–4; §3.3 cleaning → Task 2; §3.4 seed-merge → Tasks 6–7; §3.5 output → Task 6 `main`; §4 categories/targets → Tasks 6 (`MIN_TARGET`) & 7 (seeds); §5 security/deps → Task 1 + Task 5; §7 acceptance → Task 8; §8 verification → Task 8.
- **`hard_things` / `planets` have empty `SOURCES`** by design (curated-only) — `scrape_category` returns `[]`, seeds are preserved by the merge.
- **Capped categories** (`constellations`=88, `elements`=118, `planets`=13) reach target via complete seed lists in Task 7 even if scrapes add nothing.
- **No credentials** are ever written to a tracked file; Task 8 Step 4 actively greps for the old proxy string/password.
