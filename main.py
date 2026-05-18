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
        text = a.get_text(" ", strip=True)
        if text:
            out.append(text)
    return out


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
