-- Per-user feedback on a specific nickname assignment (FK → nick_changes).
-- One row per (assignment, submitter); resubmission is an UPDATE via
-- the unique constraint and ON CONFLICT DO UPDATE in upsert_feedback.

-- Speed up the "this user's most recent nick change in this guild within
-- N days" lookup used by the feedback context-menu command. The existing
-- (guild_id, changed_at DESC) index doesn't help a user-scoped query.
CREATE INDEX IF NOT EXISTS nick_changes_guild_user_idx
    ON nick_changes (guild_id, user_id, changed_at DESC);

CREATE TABLE IF NOT EXISTS feedback (
    id                    BIGSERIAL    PRIMARY KEY,
    nick_change_id        BIGINT       NOT NULL
                                         REFERENCES nick_changes(id)
                                         ON DELETE CASCADE,
    submitted_by          BIGINT       NOT NULL,
    is_relevant           BOOLEAN,                 -- NULL = no opinion / skipped
    nsfw_miscategorized   BOOLEAN      NOT NULL DEFAULT FALSE,
    note                  VARCHAR(140),
    resolved_at           TIMESTAMPTZ,             -- for the future admin panel
    created_at            TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    UNIQUE (nick_change_id, submitted_by)
);
