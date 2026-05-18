//! Framework-level command logging (poise `pre_command`/`post_command`/
//! `on_error` hooks).
//!
//! Emits structured start/end/error records to the process subscriber
//! (stderr → `docker logs`). Per-invocation timing uses a `DashMap` keyed by
//! poise's invocation id so durations survive Tokio moving the pre/post
//! futures across worker threads (a thread-local gave `duration_ms=0` in
//! production in `bot-template-rs`).

use std::sync::OnceLock;
use std::time::Instant;

use dashmap::DashMap;
use poise::FrameworkError;
use tracing::{error, info};

use crate::{Data, Error};

type Ctx<'a> = poise::Context<'a, Data, Error>;

static COMMAND_START_TIMES: OnceLock<DashMap<u64, Instant>> = OnceLock::new();

fn start_times() -> &'static DashMap<u64, Instant> {
    COMMAND_START_TIMES.get_or_init(DashMap::new)
}

fn guild_str(ctx: &Ctx<'_>) -> String {
    ctx.guild_id()
        .map_or_else(|| "DM".to_string(), |id| id.get().to_string())
}

/// `pre_command` hook: record start time and log invocation.
pub fn log_command_start(ctx: Ctx<'_>) {
    start_times().insert(ctx.id(), Instant::now());
    info!(
        command = %ctx.command().qualified_name,
        guild_id = %guild_str(&ctx),
        user_id = %ctx.author().id.get(),
        event = "start",
        "command started"
    );
}

/// `post_command` hook: log completion with elapsed duration.
pub fn log_command_end(ctx: Ctx<'_>) {
    let duration_ms = start_times()
        .remove(&ctx.id())
        .map(|(_, start)| start.elapsed())
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    info!(
        command = %ctx.command().qualified_name,
        guild_id = %guild_str(&ctx),
        user_id = %ctx.author().id.get(),
        duration_ms,
        event = "end",
        "command completed"
    );
}

/// Structured error logging. The caller still invokes
/// `poise::builtins::on_error` afterwards, so user-facing replies (permission
/// messages, argument errors, etc.) are preserved.
pub fn log_command_error(err: &FrameworkError<'_, Data, Error>) {
    match err {
        FrameworkError::Command { error, ctx, .. } => {
            error!(
                command = %ctx.command().qualified_name,
                guild_id = %guild_str(ctx),
                user_id = %ctx.author().id.get(),
                error = %error,
                "command error"
            );
        }
        FrameworkError::CommandCheckFailed { error, ctx, .. } => {
            let msg = error
                .as_ref()
                .map_or_else(|| "check failed".to_string(), ToString::to_string);
            error!(
                command = %ctx.command().qualified_name,
                guild_id = %guild_str(ctx),
                user_id = %ctx.author().id.get(),
                error = %msg,
                "command check failed"
            );
        }
        other => {
            error!(error = %other, "framework error");
        }
    }
}
