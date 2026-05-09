use anyhow::Result;
use compat_matrix::{
    run_compat_matrix, run_compat_matrix_parallel, types::ImageList, CompatReport,
};
use std::path::PathBuf;
use structopt::StructOpt;
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(Debug, StructOpt)]
#[structopt(name = "compat-matrix", about = "Docker Hub compatibility matrix runner")]
struct Args {
    #[structopt(long, default_value = "http://localhost:8080")]
    vyomad_url: String,

    #[structopt(long, default_value = "tests/compat/images.json")]
    images_file: PathBuf,

    #[structopt(long, default_value = "10")]
    parallel: usize,

    #[structopt(long)]
    output_file: Option<PathBuf>,

    #[structopt(long)]
    verbose: bool,
}

impl Args {
    fn configure_logging(&self) {
        let level = if self.verbose {
            Level::DEBUG
        } else {
            Level::INFO
        };

        let subscriber = FmtSubscriber::builder()
            .with_max_level(level)
            .with_target(false)
            .with_thread_ids(false)
            .with_file(false)
            .with_line_number(false)
            .compact()
            .finish();

        let _ = tracing::subscriber::set_global_default(subscriber);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::from_args();
    args.configure_logging();

    info!("Loading images from: {:?}", args.images_file);
    let image_list = ImageList::load(&args.images_file)?;
    info!("Loaded {} images", image_list.images.len());

    info!(
        "Starting compatibility matrix (parallel={})...",
        args.parallel
    );

    let report = if args.parallel > 1 {
        run_compat_matrix_parallel(&args.vyomad_url, image_list.images, args.parallel).await?
    } else {
        run_compat_matrix(&args.vyomad_url, image_list.images).await?
    };

    let json = report.to_json()?;
    println!("\n{}", json);

    if let Some(output_path) = &args.output_file {
        std::fs::write(output_path, &json)?;
        info!("Report written to: {:?}", output_path);
    }

    let summary = &report.summary;
    println!(
        "\n=== Compatibility Summary ===\n\
         Total: {}\n\
         Pull: {:.1}%\n\
         Boot: {:.1}%\n\
         Healthcheck: {:.1}%\n\
         Overall: {:.1}%\n\
         Passed: {} | Failed: {}",
        report.total_images,
        summary.pull_success_rate * 100.0,
        summary.boot_success_rate * 100.0,
        summary.healthcheck_success_rate * 100.0,
        summary.overall_success_rate * 100.0,
        report.passed,
        report.failed
    );

    if report.failed > 0 {
        error!("{} images failed compatibility checks", report.failed);
        std::process::exit(1);
    }

    Ok(())
}
