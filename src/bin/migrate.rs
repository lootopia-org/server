use server::config::{load_config, load_dotenv};
use server::utils::db;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    load_dotenv(".env");
    let cfg = load_config();

    println!("Applying migrations to: {}", cfg.database_url);
    let pool = db::make_pool(&cfg.database_url).await?;
    db::run_migrations(&pool).await?;
    println!("Migrations applied.");
    Ok(())
}
