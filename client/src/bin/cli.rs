use std::fs::OpenOptions;
use std::time::Duration;
use std::{collections::HashMap, path::PathBuf};

use anyhow::Context;
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use statrs::statistics::Data;
use tabled::Table;
use tokio::sync::mpsc::{self, error::TryRecvError, UnboundedReceiver};

use client::args::Args;
use client::table::ResultTableEntry;
use common::cli::{self, BenchmarkParameters, BenchmarkReport, BenchmarkUpdate};

pub const _DEFAULT_URL: &str = "http://127.0.0.1:8080/person";

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let (tx, rx) = mpsc::unbounded_channel::<BenchmarkUpdate>();

    let display_progress = tokio::spawn(display_progress(args.clone(), rx));

    println!("Running on {} ...", &args.target_url);

    let params = BenchmarkParameters {
        connections: args.num_connections,
        requests: args.num_requests,
        target_uri: args.target_url,
    };

    let benchmark_report = cli::run(&params, tx).await.expect("The benchmark failed:");

    let _ = display_progress.await;

    print_summary(&params, &benchmark_report);
    let data = calc_tabular_data(&benchmark_report);
    print_details(&data);

    if let Some(output_file) = args.output_file {
        write_csv(&output_file, &data).expect("Could not write output file");
    }
}

fn print_summary(params: &BenchmarkParameters, benchmark_report: &BenchmarkReport) {
    let BenchmarkReport {
        ok_requests,
        failed_requests,
        total_duration_ms,
        ..
    } = benchmark_report;

    let BenchmarkParameters {
        connections,
        requests,
        target_uri,
    } = params;

    println!(
        "Sent {} requests in {}ms to {} from {} connections",
        requests, total_duration_ms, target_uri, connections
    );
    println!("Performed {ok_requests} ({failed_requests} failed) requests.");
}

fn calc_tabular_data(benchmark_report: &BenchmarkReport) -> Vec<ResultTableEntry> {
    let mut microseconds_by_status_code: HashMap<u16, Vec<f64>> = HashMap::new();

    benchmark_report
        .reports
        .iter()
        .flat_map(|s| &s.requests)
        .for_each(|r| {
            microseconds_by_status_code
                .entry(r.status_code)
                .or_default()
                .push(r.duration.as_micros() as f64)
        });

    microseconds_by_status_code
        .into_iter()
        .map(|(k, v)| (k, Data::new(v)))
        .map(|(k, duration)| ResultTableEntry::new(k, duration))
        .collect::<Vec<ResultTableEntry>>()
}

fn print_details(data: &Vec<ResultTableEntry>) {
    let t = Table::new(data).to_string();
    println!("{}", t);
}

async fn display_progress(args: Args, mut rx: UnboundedReceiver<BenchmarkUpdate>) {
    let pbar = ProgressBar::new(args.num_requests);
    pbar.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {per_sec:7}",
        )
        .unwrap()
        .progress_chars("#>-"),
    );
    pbar.enable_steady_tick(Duration::from_millis(100));
    let mut requests_per_connections = vec![0u64; args.num_connections as usize];
    loop {
        match rx.try_recv() {
            Ok(update) => {
                requests_per_connections[update.connection_id as usize] = update.current_request;
                pbar.set_position(requests_per_connections.iter().sum())
            }
            Err(TryRecvError::Empty) => {
                pbar.tick();
                tokio::time::sleep(Duration::from_millis(50)).await
            }
            Err(TryRecvError::Disconnected) => {
                break;
            }
        }
    }

    pbar.finish_and_clear();
}

fn write_csv(path: &PathBuf, data: &Vec<ResultTableEntry>) -> anyhow::Result<()> {
    println!("Saving results to {}", path.to_str().context("Wrong path")?);

    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .context("File already exists")?;
    let mut wtr = csv::Writer::from_writer(file);
    for entry in data {
        wtr.serialize(entry).context("Failed to write csv file")?;
    }
    wtr.flush().context("Failed to flush csv file")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use client::args::Args;

    #[test]
    fn test_works_with_url() {
        let args = Args::try_parse_from([
            "loadcli",
            "https://some.host.example.com:4242/uri?param=foo&bar=bazz",
        ]);
        assert!(args.is_ok());
    }

    #[test]
    fn test_invalid_connections_argument() {
        let args = Args::try_parse_from(["loadcli", "-c", "0"]);
        assert!(args.is_err());
    }

    #[test]
    fn test_invalid_target_url_argument() {
        let args = Args::try_parse_from(["loadcli", "invalid_url//"]);
        assert!(args.is_err());
    }
}
