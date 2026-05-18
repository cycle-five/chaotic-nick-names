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
