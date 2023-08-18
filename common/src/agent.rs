use std::time::Duration;

use hyper::{Client, Uri};
use tokio::{
    select,
    sync::{mpsc, watch},
    time::{interval, timeout, Instant},
};

use crate::{RequestReport, StatusOnlyHttpClient};

const REQ_TIMEOUT: u64 = 500;

#[derive(Clone)]
pub struct BenchmarkParameters {
    pub connections: u64,
    pub target_uri: Uri,
    pub interval_ms: u64,
}

struct ConnectionParameters {
    pub connection_id: u64,
    pub target_uri: Uri,
    pub interval_ms: u64,
}

#[derive(Debug)]
pub enum RequestUpdate {
    Success(RequestReport),
    Failure,
    Timeout,
}

pub async fn run(
    params: &BenchmarkParameters,
    tx_update: mpsc::UnboundedSender<RequestUpdate>,
    rx_terminate: watch::Receiver<bool>,
) {
    let BenchmarkParameters {
        connections,
        target_uri,
        interval_ms,
    } = params;

    let _handles = (0..*connections)
        .map(|id| {
            let params = ConnectionParameters {
                connection_id: id,
                target_uri: target_uri.clone(),
                interval_ms: *interval_ms,
            };
            tokio::spawn(connection_task(
                Client::builder().build_http(),
                params,
                tx_update.clone(),
                rx_terminate.clone(),
            ))
        })
        .collect::<Vec<_>>();
}

async fn connection_task(
    client: impl StatusOnlyHttpClient,
    params: ConnectionParameters,
    tx_update: mpsc::UnboundedSender<RequestUpdate>,
    mut rx_terminate: watch::Receiver<bool>,
) {
    let mut interval = interval(Duration::from_millis(params.interval_ms));
    while let Ok(false) = rx_terminate.has_changed() {
        do_request(&client, &params.target_uri, &tx_update).await;
        select! {
            _ = interval.tick() => {
                // just continue
            }
            _ = rx_terminate.changed() => {
                break;
            }
        };
    }
    log::info!("Terminating connection {}", params.connection_id);
}

pub async fn do_request(
    client: &impl StatusOnlyHttpClient,
    target_uri: &Uri,
    tx_update: &mpsc::UnboundedSender<RequestUpdate>,
) {
    let request_future = client.get(target_uri.clone());
    let start_instant = Instant::now();
    let result = timeout(Duration::from_millis(REQ_TIMEOUT), request_future).await;
    let duration = Instant::now().duration_since(start_instant);

    let request_update = match result {
        Ok(Ok(status_code)) => RequestUpdate::Success(RequestReport {
            status_code,
            duration,
        }),
        Ok(Err(_)) => RequestUpdate::Failure,
        Err(_) => RequestUpdate::Timeout,
    };

    // TODO fix unwrap
    tx_update
        .send(request_update)
        .expect("The result channel was closed before the connection was done!");
}
