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


def test_extract_links_real_wiki_h2_with_editsection():
    html = """<html><body><div class="mw-parser-output">
      <p><a href="/wiki/RealArticle">RealArticle</a></p>
      <h2><span class="mw-headline" id="See_also">See also</span>
          <span class="mw-editsection">[<a href="#">edit</a>]</span></h2>
      <ul><li><a href="/wiki/SeeAlso">SeeAlso</a></li></ul>
    </div></body></html>"""
    got = extract_links(html, {})
    assert "RealArticle" in got
    assert "SeeAlso" not in got


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
