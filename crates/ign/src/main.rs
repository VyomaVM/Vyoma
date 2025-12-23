use tracing::{info, debug};
use tracing_subscriber::FmtSubscriber;

fn main() {
    // Initialize logging
    // For CLI, we might want to default to cleaner output, but for dev, full logs are fine.
    let subscriber = FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed");

    debug!("Ignite CLI initializing...");
    info!("Ignite CLI (ign) - v0.1.0");
    
    // Placeholder command dispatch
    println!("Hello from ign! Use --help (once implemented) for commands.");
}
