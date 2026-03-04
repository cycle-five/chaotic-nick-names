use std::sync::Arc;
use tokio::sync::RwLock;

use poise::serenity_prelude as serenity;

mod commands;
mod data;
mod db;
mod state;

/// Shared application data: in-memory state + Postgres connection pool.
pub struct BotData {
    pub state: RwLock<state::AppState>,
    pub db: sqlx::PgPool,
}

impl BotData {
    pub async fn read_state(&self) -> tokio::sync::RwLockReadGuard<'_, state::AppState> {
        self.state.read().await
    }

    pub async fn write_state(&self) -> tokio::sync::RwLockWriteGuard<'_, state::AppState> {
        self.state.write().await
    }
}

/// Shared application state threaded through every command.
pub type Data = Arc<BotData>;

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

    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set in the environment");

    // Connect to Postgres and run migrations
    let pool = db::setup(&database_url)
        .await
        .expect("Failed to connect to Postgres / run migrations");
    tracing::info!("Database ready.");

    // Pre-load all guild states from the database
    let guilds = db::load_all_guilds(&pool)
        .await
        .expect("Failed to load guild states from the database");
    tracing::info!("Loaded {} guild(s) from the database.", guilds.len());

    let app_state = state::AppState::from_guilds(guilds);

    let bot_data: Data = Arc::new(BotData {
        state: RwLock::new(app_state),
        db: pool,
    });

    // GUILD_MEMBERS is a privileged intent — enable it in the Discord developer portal
    let intents =
        serenity::GatewayIntents::GUILDS | serenity::GatewayIntents::GUILD_MEMBERS;

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
                Ok(bot_data)
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
