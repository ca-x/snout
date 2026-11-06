#[tokio::main]
async fn main() -> std::process::ExitCode {
    snout::cli::run().await
}
