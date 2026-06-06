use anyhow::Context;
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};

pub async fn make_pool(database_url: &str) -> anyhow::Result<PgPool> {
    let options = connect_options(database_url)?;
    PgPoolOptions::new()
        .max_connections(10)
        .connect_with(options)
        .await
        .context("connecting to PostgreSQL")
}

pub async fn run_migrations(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::migrate!()
        .run(pool)
        .await
        .context("running migrations")
}

fn connect_options(database_url: &str) -> anyhow::Result<PgConnectOptions> {
    let trimmed = database_url.trim();
    if trimmed.starts_with("postgres://") || trimmed.starts_with("postgresql://") {
        return trimmed.parse().context("parsing DATABASE_URL");
    }

    let mut options = PgConnectOptions::new();
    for pair in trimmed.split_whitespace() {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        options = match key {
            "host" => options.host(value),
            "hostaddr" => options.host(value),
            "port" => options.port(value.parse().context("parsing DATABASE_URL port")?),
            "dbname" | "database" => options.database(value),
            "user" => options.username(value),
            "password" => options.password(value),
            _ => options,
        };
    }
    Ok(options)
}
