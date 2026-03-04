-- Custom categories stored per guild.
-- Each row holds the full list of names as a Postgres text array.
CREATE TABLE IF NOT EXISTS custom_categories (
    guild_id   BIGINT  NOT NULL,
    name       TEXT    NOT NULL,
    items      TEXT[]  NOT NULL,
    PRIMARY KEY (guild_id, name)
);

-- Without-replacement pool tracking: which names have already been handed out.
CREATE TABLE IF NOT EXISTS used_names (
    guild_id      BIGINT  NOT NULL,
    category_name TEXT    NOT NULL,
    name          TEXT    NOT NULL,
    PRIMARY KEY (guild_id, category_name, name)
);

-- Full nickname-change history (up to 200 per guild kept in memory; DB stores all).
CREATE TABLE IF NOT EXISTS nick_changes (
    id          BIGSERIAL    PRIMARY KEY,
    guild_id    BIGINT       NOT NULL,
    user_id     BIGINT       NOT NULL,
    user_name   TEXT         NOT NULL,
    old_nick    TEXT,
    new_nick    TEXT         NOT NULL,
    category    TEXT         NOT NULL,
    changed_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS nick_changes_guild_idx
    ON nick_changes (guild_id, changed_at DESC);

-- Aggregate statistics per guild.
CREATE TABLE IF NOT EXISTS guild_stats (
    guild_id             BIGINT  PRIMARY KEY,
    total_changes        BIGINT  NOT NULL DEFAULT 0,
    bulk_randomize_count BIGINT  NOT NULL DEFAULT 0
);

-- Per-category usage counters per guild.
CREATE TABLE IF NOT EXISTS category_usage (
    guild_id      BIGINT  NOT NULL,
    category_name TEXT    NOT NULL,
    usage_count   BIGINT  NOT NULL DEFAULT 0,
    PRIMARY KEY (guild_id, category_name)
);
