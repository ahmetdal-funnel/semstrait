#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    semstrait_api::cli::run().await
}
