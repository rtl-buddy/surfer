//! Code for the `surver` executable.
use clap::Parser;
use eyre::{Context, Result, bail};
use std::{
    fs::File,
    io::{BufRead, BufReader, stdout},
    path::Path,
};
use tokio::runtime::Builder;
use tracing::subscriber::set_global_default;
use tracing_subscriber::{EnvFilter, Layer, Registry, fmt, layer::SubscriberExt};

#[derive(Parser, Default)]
#[command(version = concat!(env!("CARGO_PKG_VERSION"), " (git: ", env!("VERGEN_GIT_DESCRIBE"), ")"), about)]
struct Args {
    #[clap(flatten)]
    file_group: FileGroup,
    /// Port on which server will listen
    #[clap(long)]
    port: Option<u16>,
    /// IP address to bind the server to
    #[clap(long)]
    bind_address: Option<String>,
    /// Token used by the client to authenticate to the server
    #[clap(long)]
    token: Option<String>,
}

#[derive(Debug, Default, clap::Args)]
#[group(required = true)]
pub struct FileGroup {
    /// Waveform files in VCD, FST, or GHW format.
    wave_files: Vec<String>,
    /// File with one wave form file name per line
    #[clap(long)]
    file: Option<String>,
}

/// Starts the logging and error handling. Can be used by unittests to get more insights.
pub fn start_logging() -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
    let subscriber = Registry::default().with(
        fmt::layer()
            .without_time()
            .with_writer(stdout)
            .with_filter(filter.clone()),
    );

    set_global_default(subscriber).expect("unable to set global subscriber");

    Ok(())
}

/// Load list of file names from a file (one per line)
fn load_file_list(filename: &str) -> Result<Vec<String>> {
    let file =
        File::open(filename).with_context(|| format!("Failed to open file list: {}", filename))?;
    let buf = BufReader::new(file);
    buf.lines()
        .map(|l| l.with_context(|| format!("Failed to read line from: {}", filename)))
        .filter(|result| result.as_ref().map(|s| !s.is_empty()).unwrap_or(true))
        .collect()
}

/// Validate that all files exist and are readable
fn validate_files(filenames: &[String]) -> Result<()> {
    for filename in filenames {
        let path = Path::new(filename);
        if !path.exists() {
            bail!("Wave file does not exist: {}", filename);
        }
        if !path.is_file() {
            bail!("Path is not a file: {}", filename);
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    start_logging()?;

    let runtime = Builder::new_current_thread()
        .worker_threads(1)
        .enable_all()
        .build()?;

    // parse arguments
    let args = Args::parse();

    // Collect file names from direct arguments and file list
    let mut file_names = args.file_group.wave_files;
    if let Some(filename) = args.file_group.file {
        file_names.append(&mut load_file_list(&filename)?);
    }

    // Validate that all files exist before starting server
    validate_files(&file_names)?;

    // Use CLI override if provided, otherwise use hardcoded defaults
    let bind_addr = args
        .bind_address
        .unwrap_or_else(|| std::net::Ipv4Addr::LOCALHOST.to_string());
    let port = args.port.unwrap_or(8911);

    runtime.block_on(surver::surver_main(
        port,
        bind_addr,
        args.token,
        &file_names,
        None,
    ))
}
