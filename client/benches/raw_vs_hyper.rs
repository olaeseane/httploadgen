use std::str::FromStr;

use criterion::{criterion_group, criterion_main, Criterion};
use hyper::{Client, Uri};
use tokio::{net::TcpStream, runtime::Runtime, sync::mpsc};

use common::cli::{BenchmarkUpdate, ConnectionReport};

criterion_group!(benches, bench_http_requests);
criterion_main!(benches);

fn bench_http_requests(c: &mut Criterion) {
    let requests = 1_000;
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("http_requests");

    group.bench_function("do_raw_requests", |b| {
        b.to_async(&rt)
            .iter(|| do_connect_and_request_raw(requests));
    });

    group.bench_function("do_hyper_requests", |b| {
        b.to_async(&rt)
            .iter(|| do_connect_and_request_hyper(requests));
    });
}

async fn do_connect_and_request_raw(requests: u64) {
    let client = TcpStream::connect("127.0.0.1:8080").await.unwrap();
    common::do_request_raw(client, requests).await;
}
async fn do_connect_and_request_hyper(requests: u64) {
    let client = Client::builder().build_http();
    let mut conn_report = ConnectionReport::new(1, requests);

    let (tx, _) = mpsc::unbounded_channel::<BenchmarkUpdate>();

    for n in 0..requests {
        let _ = common::cli::do_request(
            &client,
            &Uri::from_str("http://127.0.0.1:8080/person").unwrap(),
            &mut conn_report,
            n,
            &tx,
        )
        .await;
    }
}
