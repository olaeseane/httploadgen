use std::time::Duration;

use clap::Parser;
use lazy_static::lazy_static;
use prometheus::{
    histogram_opts, register_histogram_vec_with_registry, register_int_counter_vec_with_registry,
    Encoder, HistogramVec, IntCounterVec, Registry, TextEncoder,
};
use tokio::{
    select,
    sync::{
        mpsc::{self, UnboundedReceiver},
        watch,
    },
    time::interval,
};

use client::args::Args;
use common::agent::{self, BenchmarkParameters, RequestUpdate};

lazy_static! {
    static ref REG: Registry = Registry::new_custom(Some("loadcli".to_string()), None).unwrap();
    static ref REQUEST_LATENCY_HIST: HistogramVec = register_histogram_vec_with_registry!(
        histogram_opts!(
            "latency_histogram",
            "latency historgram of observed requests",
            vec![1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0, 256.0, 512.0, 1024.0, 2048.0, 4096.0]
        ),
        &["code"],
        REG
    )
    .unwrap();
    static ref REQ_COUNTERS: IntCounterVec = register_int_counter_vec_with_registry!(
        "req_counter",
        "counter for requests",
        &["outcome"],
        REG
    )
    .unwrap();
}

#[tokio::main]
async fn main() {
    env_logger::init();

    tokio::spawn(listen_metrics());

    let args = Args::parse();

    let (tx_terminate, rx_terminate) = watch::channel(true);

    ctrlc::set_handler(move || {
        tx_terminate
            .send(false)
            .expect("Could not send signal on channel.")
    })
    .expect("Error setting Ctrl-C handler");

    let (tx_update, rx_update) = mpsc::unbounded_channel::<RequestUpdate>();

    let receive_progress_handle = tokio::spawn(receive_progress(args.clone(), rx_update));

    log::info!("Running on {} ...", &args.target_url);

    let bench_parameters = BenchmarkParameters {
        connections: args.num_connections,
        target_uri: args.target_url,
        interval_ms: args.interval_ms,
    };

    agent::run(&bench_parameters, tx_update, rx_terminate).await;

    let _ = receive_progress_handle.await;
}

async fn listen_metrics() {
    let app = axum::Router::new().route("/metrics", axum::routing::get(get_metrics));
    log::info!("Metrics on {}", "0.0.0.0:8001/metrics");
    axum::Server::bind(&"0.0.0.0:8001".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn get_metrics() -> String {
    let mut buffer = vec![];
    let encoder = TextEncoder::new();
    let metric_families = REG.gather();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap()
}

async fn receive_progress(_args: Args, mut rx: UnboundedReceiver<RequestUpdate>) {
    let print_interval = 2000;

    let mut observation_count = 0;
    let mut aggregated_latency_us = 0;
    let mut last_observation_count = 0;
    let mut last_aggregated_latency_us = 0;
    let mut interval = interval(Duration::from_millis(print_interval));

    loop {
        select! {
            _ = interval.tick() => {
                // TODO handle ticked
                let amount = observation_count - last_observation_count;
                if amount > 0 {
                    let latency_sum = aggregated_latency_us - last_aggregated_latency_us;
                    let average_latency = latency_sum / amount;
                    log::info!("average_latency: {average_latency}us")
                }
                last_observation_count = observation_count;
                last_aggregated_latency_us = aggregated_latency_us;
            }
            update = rx.recv() => {
                // handle received update
                match update {
                    Some(update) => match update {
                        RequestUpdate::Success(res) => {
                            observation_count += 1;
                            aggregated_latency_us += res.duration.as_micros();
                            REQ_COUNTERS.with_label_values(&["Successful"]).inc();
                            REQUEST_LATENCY_HIST
                                .with_label_values(&[&res.status_code.to_string()])
                                .observe(res.duration.as_micros() as f64 / 1000.0);
                            log::debug!("Observed request: {:?}", res);
                        }
                        RequestUpdate::Failure => {
                            REQ_COUNTERS.with_label_values(&["Failure"]).inc();
                            log::warn!("Observed failure: {:?}", update);
                        }
                        RequestUpdate::Timeout => {
                            REQ_COUNTERS.with_label_values(&["Timeout"]).inc();
                            log::warn!("Observed timeout: {:?}", update);
                        }
                    },
                    None => {
                        log::info!("Terminating printer");
                        return;
                    }
                }
            }
        }
    }
}
