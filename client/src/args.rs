use std::{ops::RangeInclusive, path::PathBuf, str::FromStr};

use clap::Parser;
use hyper::Uri;

#[derive(Parser, Clone)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[arg(value_name = "url", value_parser = is_url_valid, env)]
    pub target_url: Uri,
    #[arg(short = 'c', long = "connections", default_value_t = 10, value_parser = in_range, env)]
    pub num_connections: u64,
    #[arg(short = 'r', long = "requests", default_value_t = 5000, value_parser = clap::value_parser!(u64).range(1..))]
    pub num_requests: u64,
    #[arg(short = 'f', long = "file")]
    pub output_file: Option<PathBuf>,
    #[arg(short, long, default_value_t = 1000, env)]
    pub interval_ms: u64,
}

const IN_RANGE: RangeInclusive<usize> = 1..=65535;

fn in_range(s: &str) -> Result<u64, String> {
    let numbers: usize = s.parse().map_err(|_| format!("`{s}` isn't a numbers"))?;
    if IN_RANGE.contains(&numbers) {
        Ok(numbers as u64)
    } else {
        Err(format!(
            "numbers {} not in range {}-{}",
            s,
            IN_RANGE.start(),
            IN_RANGE.end()
        ))
    }
}

fn is_url_valid(s: &str) -> Result<Uri, String> {
    Uri::from_str(s).map_err(|uri| (format!("{s} {uri}")))
}

/*

fn is_path_valid(s: &str) -> Result<PathBuf, String> {
    PathBuf::from_str(s).map_err(|path| format!("{s} {path}"))
}

const NUM_CON_RANGE: RangeInclusive<usize> = 1..=65536 - 10;
fn connection_in_range(s: &str) -> Result<u16, String> {
    s.parse()
        .iter()
        .filter(|i| NUM_CON_RANGE.contains(i))
        .map(|i| *i as u16)
        .next()
        .ok_or(format!(
            "Number of connection not in range {}-{}",
            NUM_CON_RANGE.start(),
            NUM_CON_RANGE.end()
        ))
}
 */
