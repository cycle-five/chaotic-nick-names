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
