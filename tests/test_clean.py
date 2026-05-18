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
