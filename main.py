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
