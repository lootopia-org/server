#[tokio::main]
async fn main() -> anyhow::Result<()> {
    server::server::run().await
}
