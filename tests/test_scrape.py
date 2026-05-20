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


def test_main_only_updates_selected_categories(tmp_path, monkeypatch):
    cats = tmp_path / "categories.json"
    cats.write_text(
        json.dumps({"cocktails": ["Negroni"], "spices": ["Cumin"]}),
        encoding="utf-8")
    monkeypatch.setattr(m, "CATEGORIES_PATH", cats)
    monkeypatch.setattr(m, "get_session", lambda: object())
    monkeypatch.setattr(
        m, "scrape_category",
        lambda session, name: ["Sazerac"] if name == "cocktails"
        else ["Sumac"])

    m.main(only=["cocktails"])

    data = json.loads(cats.read_text(encoding="utf-8"))
    assert "Sazerac" in data["cocktails"]      # selected category updated
    assert data["spices"] == ["Cumin"]          # untouched category preserved


def test_main_replace_discards_existing_for_selected(tmp_path, monkeypatch):
    cats = tmp_path / "categories.json"
    cats.write_text(
        json.dumps({"cocktails": ["Negroni", "Martini"],
                    "spices": ["Cumin"]}),
        encoding="utf-8")
    monkeypatch.setattr(m, "CATEGORIES_PATH", cats)
    monkeypatch.setattr(m, "get_session", lambda: object())
    monkeypatch.setattr(
        m, "scrape_category",
        lambda session, name: ["Sazerac", "Aviation"]
        if name == "cocktails" else [])

    m.main(only=["cocktails"], replace=True)

    data = json.loads(cats.read_text(encoding="utf-8"))
    # Seeds gone; only scrape survives, deduped + cleaned.
    assert data["cocktails"] == ["Sazerac", "Aviation"]
    # Non-selected category untouched.
    assert data["spices"] == ["Cumin"]


def test_main_replace_requires_only(tmp_path, monkeypatch, capsys):
    cats = tmp_path / "categories.json"
    original = {"cocktails": ["Negroni", "Martini"]}
    cats.write_text(json.dumps(original), encoding="utf-8")
    monkeypatch.setattr(m, "CATEGORIES_PATH", cats)

    m.main(replace=True)  # no --only

    # File untouched, error printed.
    assert json.loads(cats.read_text(encoding="utf-8")) == original
    out = capsys.readouterr().out
    assert "--replace requires --only" in out


def test_main_replace_refuses_empty_scrape(tmp_path, monkeypatch, capsys):
    cats = tmp_path / "categories.json"
    cats.write_text(
        json.dumps({"cocktails": ["Negroni", "Martini"]}),
        encoding="utf-8")
    monkeypatch.setattr(m, "CATEGORIES_PATH", cats)
    monkeypatch.setattr(m, "get_session", lambda: object())
    monkeypatch.setattr(m, "scrape_category", lambda session, name: [])

    m.main(only=["cocktails"], replace=True)

    # Existing entries preserved exactly.
    data = json.loads(cats.read_text(encoding="utf-8"))
    assert data["cocktails"] == ["Negroni", "Martini"]
    out = capsys.readouterr().out
    assert "refusing to wipe" in out


def test_main_replace_allow_empty_writes_empty(tmp_path, monkeypatch):
    cats = tmp_path / "categories.json"
    cats.write_text(
        json.dumps({"cocktails": ["Negroni"]}),
        encoding="utf-8")
    monkeypatch.setattr(m, "CATEGORIES_PATH", cats)
    monkeypatch.setattr(m, "get_session", lambda: object())
    monkeypatch.setattr(m, "scrape_category", lambda session, name: [])

    m.main(only=["cocktails"], replace=True, allow_empty=True)

    data = json.loads(cats.read_text(encoding="utf-8"))
    assert data["cocktails"] == []


def test_main_dry_run_does_not_write(tmp_path, monkeypatch, capsys):
    cats = tmp_path / "categories.json"
    original = {"cocktails": ["Negroni"], "spices": ["Cumin"]}
    cats.write_text(json.dumps(original), encoding="utf-8")
    monkeypatch.setattr(m, "CATEGORIES_PATH", cats)
    monkeypatch.setattr(m, "get_session", lambda: object())
    monkeypatch.setattr(
        m, "scrape_category",
        lambda session, name: ["Sazerac"] if name == "cocktails" else [])

    m.main(only=["cocktails"], dry_run=True)

    # File contents byte-identical — dry run wrote nothing.
    assert json.loads(cats.read_text(encoding="utf-8")) == original
    out = capsys.readouterr().out
    assert "DRY RUN" in out
    assert "+ Sazerac" in out      # diff shows the prospective add
    assert "NOT modified" in out


def test_main_from_file_txt_additive(tmp_path, monkeypatch):
    cats = tmp_path / "categories.json"
    cats.write_text(
        json.dumps({"cocktails": ["Negroni"]}), encoding="utf-8")
    overrides = tmp_path / "cocktails.txt"
    overrides.write_text(
        "# curated overrides\nSazerac\nAviation\n\n# blank line above\n",
        encoding="utf-8")
    monkeypatch.setattr(m, "CATEGORIES_PATH", cats)
    monkeypatch.setattr(m, "get_session", lambda: object())
    # If scrape_category is called for cocktails, fail loudly — --from-file
    # must completely bypass Wikipedia for that category.
    def _scrape_should_not_be_called(session, name):
        raise AssertionError(
            f"scrape_category called for {name!r} despite --from-file")
    monkeypatch.setattr(m, "scrape_category", _scrape_should_not_be_called)

    m.main(from_file={"cocktails": overrides})

    data = json.loads(cats.read_text(encoding="utf-8"))
    # Additive: seed preserved, file contents added, comments + blanks ignored.
    assert data["cocktails"][0] == "Negroni"
    assert "Sazerac" in data["cocktails"]
    assert "Aviation" in data["cocktails"]
    assert "# curated overrides" not in data["cocktails"]
    assert "" not in data["cocktails"]


def test_main_from_file_json_format(tmp_path, monkeypatch):
    cats = tmp_path / "categories.json"
    cats.write_text(json.dumps({"cocktails": []}), encoding="utf-8")
    overrides = tmp_path / "cocktails.json"
    overrides.write_text(json.dumps(["Sazerac", "Aviation"]),
                         encoding="utf-8")
    monkeypatch.setattr(m, "CATEGORIES_PATH", cats)
    monkeypatch.setattr(m, "get_session", lambda: object())
    monkeypatch.setattr(m, "scrape_category",
                       lambda s, n: (_ for _ in ()).throw(
                           AssertionError("should not scrape")))

    m.main(from_file={"cocktails": overrides})

    data = json.loads(cats.read_text(encoding="utf-8"))
    assert "Sazerac" in data["cocktails"]
    assert "Aviation" in data["cocktails"]


def test_main_from_file_with_replace_wipes_seed(tmp_path, monkeypatch):
    cats = tmp_path / "categories.json"
    cats.write_text(
        json.dumps({"cocktails": ["Negroni", "Martini"]}), encoding="utf-8")
    overrides = tmp_path / "cocktails.txt"
    overrides.write_text("Sazerac\nAviation\n", encoding="utf-8")
    monkeypatch.setattr(m, "CATEGORIES_PATH", cats)
    monkeypatch.setattr(m, "get_session", lambda: object())
    monkeypatch.setattr(m, "scrape_category",
                       lambda s, n: (_ for _ in ()).throw(
                           AssertionError("should not scrape")))

    m.main(from_file={"cocktails": overrides}, replace=True)

    data = json.loads(cats.read_text(encoding="utf-8"))
    # Seeds gone; only file contents survive.
    assert data["cocktails"] == ["Sazerac", "Aviation"]


def test_main_replace_accepts_from_file_without_only(tmp_path, monkeypatch):
    """--replace requires --only OR --from-file; the latter alone is enough."""
    cats = tmp_path / "categories.json"
    cats.write_text(
        json.dumps({"cocktails": ["Negroni"]}), encoding="utf-8")
    overrides = tmp_path / "cocktails.txt"
    overrides.write_text("Sazerac\n", encoding="utf-8")
    monkeypatch.setattr(m, "CATEGORIES_PATH", cats)
    monkeypatch.setattr(m, "get_session", lambda: object())
    monkeypatch.setattr(m, "scrape_category",
                       lambda s, n: (_ for _ in ()).throw(
                           AssertionError("should not scrape")))

    # No --only passed; --from-file alone scopes the replacement.
    m.main(from_file={"cocktails": overrides}, replace=True)

    data = json.loads(cats.read_text(encoding="utf-8"))
    assert data["cocktails"] == ["Sazerac"]
