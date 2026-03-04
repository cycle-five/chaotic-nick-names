use std::sync::Arc;
use tokio::sync::RwLock;

use poise::serenity_prelude as serenity;

mod commands;
mod data;
mod state;

/// Shared application state threaded through every command.
pub type Data = Arc<RwLock<state::AppState>>;

/// Unified error type used by all command handlers.
pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// Poise context parameterised over our data / error types.
pub type Context<'a> = poise::Context<'a, Data, Error>;

#[tokio::main]
async fn main() {
    // Load optional .env file (silently ignore if absent)
    dotenv::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "chaotic_nick_names=info,warn".parse().unwrap()),
        )
        .init();

    let token =
        std::env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN must be set in the environment");

    // GUILD_MEMBERS is a privileged intent — enable it in the Discord developer portal
    let intents =
        serenity::GatewayIntents::GUILDS | serenity::GatewayIntents::GUILD_MEMBERS;

    let app_state: Data = Arc::new(RwLock::new(state::AppState::new()));

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: commands::all_commands(),
            on_error: |err| {
                Box::pin(async move {
                    if let Err(e) = poise::builtins::on_error(err).await {
                        tracing::error!("Unhandled error in error handler: {}", e);
                    }
                })
            },
            ..Default::default()
        })
        .setup(move |ctx, ready, framework| {
            Box::pin(async move {
                tracing::info!("Logged in as {}", ready.user.name);
                tracing::info!("Registering global application commands...");
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                tracing::info!("Commands registered successfully.");
                Ok(app_state)
            })
        })
        .build();

    let mut client = serenity::ClientBuilder::new(token, intents)
        .framework(framework)
        .await
        .expect("Failed to create Discord client");

    tracing::info!("Bot is starting...");
    if let Err(e) = client.start().await {
        tracing::error!("Client error: {:?}", e);
    }
}
