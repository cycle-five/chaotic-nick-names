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
    assert clean_name("A") is None  # single letters caught by the len<3 gate
    assert clean_name("Z") is None


def test_strips_disallowed_characters():
    assert clean_name("AC/DC!!") == "ACDC"
    assert clean_name("???") is None


def test_dedupe_keep_first_case_insensitive():
    assert dedupe_keep_first(["Gin", "gin", "GIN", "Rum"]) == ["Gin", "Rum"]
    assert dedupe_keep_first([]) == []


def test_cuts_at_em_dash():
    assert clean_name("Cosmos — a classic cocktail") == "Cosmos"


def test_dedupe_keep_first_preserves_order():
    assert dedupe_keep_first(["Rum", "Gin", "rum"]) == ["Rum", "Gin"]


def test_rejects_wikipedia_list_and_meta_junk():
    assert clean_name("List of named alloys") is None
    assert clean_name("Lists of integrals") is None
    assert clean_name("Index of physics articles") is None
    assert clean_name("Outline of chemistry") is None
    assert clean_name("Timeline of chemistry") is None
    assert clean_name("History of mathematics") is None
    assert clean_name("Glossary of chemistry terms") is None
    assert clean_name("Comparison of dinosaurs") is None
    assert clean_name("Bibliography of biology") is None
    assert clean_name("Table of nuclides") is None
    assert clean_name("Automotive industry in Pakistan") is None


def test_junk_filter_keeps_legitimate_lookalikes():
    assert clean_name("Listeria") == "Listeria"
    assert clean_name("Industry") == "Industry"
    assert clean_name("Historia") == "Historia"
    assert clean_name("Tablet") == "Tablet"


def test_junk_exceptions_whitelist():
    assert clean_name("History of the World") == "History of the World"
    assert clean_name("History of mathematics") is None  # still filtered
