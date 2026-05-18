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
