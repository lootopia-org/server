use server::auth::db;
use server::config::load_dotenv;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    config::load_dotenv(".env");
    let cfg = config::load_config();

    println!("Applying migrations to: {}", cfg.database_url);
    let pool = db::make_pool(&cfg.database_url).await?;
    db::run_migrations(&pool).await?;
    println!("Migrations applied.");
    Ok(())
}
