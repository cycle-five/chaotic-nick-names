-- Background /randomize summaries we could not deliver to the invoking user at
-- the time: the slash interaction token had expired (very large guild whose run
-- outlasts Discord's 15-minute window) AND a fallback DM also failed (their DMs
-- are closed). Rather than drop the result to a log line, we park it here and
-- surface it — deleting the row — the next time that user runs /randomize in the
-- guild. See commands::randomize_delivery and
-- db::{insert_undelivered_summary, take_undelivered_summaries}.
CREATE TABLE IF NOT EXISTS undelivered_summaries (
    id          BIGSERIAL    PRIMARY KEY,
    guild_id    BIGINT       NOT NULL,
    user_id     BIGINT       NOT NULL,
    summary     TEXT         NOT NULL,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- Delivery is a fetch-and-clear scoped to (guild, user), oldest first.
CREATE INDEX IF NOT EXISTS undelivered_summaries_guild_user_idx
    ON undelivered_summaries (guild_id, user_id, created_at);
