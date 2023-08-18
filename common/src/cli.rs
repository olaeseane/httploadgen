use std::time::Duration;

use anyhow::Context;
use hyper::{Client, Uri};
use tokio::{sync::mpsc, time::Instant};

use crate::{RequestReport, StatusOnlyHttpClient};

#[derive(Debug)]
pub struct BenchmarkUpdate {
    pub connection_id: u64,
    pub current_request: u64,
}

pub struct BenchmarkParameters {
    pub connections: u64,
    pub requests: u64,
    pub target_uri: Uri,
}

pub struct ConnectionParameters {
    pub connection_id: u64,
    pub target_uri: Uri,
    pub num_requests: u64,
}

impl ConnectionParameters {
    pub fn new(connection_id: u64, target_uri: Uri, num_requests: u64) -> Self {
        ConnectionParameters {
            connection_id,
            target_uri,
            num_requests,
        }
    }
}

pub struct BenchmarkReport {
    pub reports: Vec<ConnectionReport>,
    pub ok_requests: u64,
    pub failed_requests: u64,
    pub max_duration_ms: u64,
    pub total_duration_ms: u64,
}

#[derive(Debug)]
pub struct ConnectionReport {
    pub connection_id: u64,
    pub num_requests: u64,
    pub ok_requests: u64,
    pub failed_requests: u64,
    pub duration: Duration,
    pub requests: Vec<RequestReport>,
}

impl ConnectionReport {
    pub fn new(connection_id: u64, num_requests: u64) -> Self {
        ConnectionReport {
            connection_id,
            num_requests,
            ok_requests: 0,
            failed_requests: 0,
            duration: Duration::default(),
            requests: Vec::with_capacity(num_requests as usize),
        }
    }
}

pub async fn run(
    params: &BenchmarkParameters,
    tx_update: mpsc::UnboundedSender<BenchmarkUpdate>,
) -> anyhow::Result<BenchmarkReport> {
    let BenchmarkParameters {
        connections,
        requests,
        target_uri,
    } = params;

    let mut clients = Vec::with_capacity(*connections as usize);
    let mut handles = Vec::with_capacity(*connections as usize);
    let mut reports = Vec::with_capacity(*connections as usize);

    for _ in 0..*connections {
        clients.push(Client::builder().build_http());
    }

    let number_of_connection_with_one_more_requests =
        (params.requests % params.connections) as usize;

    let start_instant = Instant::now();

    for (id, c) in clients.into_iter().enumerate() {
        let mut param =
            ConnectionParameters::new(id as u64, target_uri.clone(), requests / connections);
        if id < number_of_connection_with_one_more_requests {
            param.num_requests += 1;
        }
        let h = tokio::spawn(connection_task(c, param, tx_update.clone()));
        handles.push(h);
    }

    for h in handles {
        let await_result = h.await;
        let connection_result = await_result.context("Failed to await for task")?;
        let report = connection_result.context("A connection failed")?;
        reports.push(report);
    }

    let total_duration_ms = start_instant.elapsed().as_millis() as u64;

    let (ok_requests, failed_requests, max_duration_ms) = calc_stats(&reports);

    Ok(BenchmarkReport {
        reports,
        ok_requests,
        failed_requests,
        max_duration_ms,
        total_duration_ms,
    })
}

pub fn calc_stats(results: &[ConnectionReport]) -> (u64, u64, u64) {
    let ok_requests = results.iter().map(|r| r.ok_requests).sum();
    let failed_requests = results.iter().map(|r| r.failed_requests).sum();

    let max_duration = results
        .iter()
        .map(|r| r.duration.as_millis())
        .max()
        .unwrap() as u64;

    (ok_requests, failed_requests, max_duration)
}

pub async fn connection_task(
    client: impl StatusOnlyHttpClient,
    params: ConnectionParameters,
    tx_update: mpsc::UnboundedSender<BenchmarkUpdate>,
) -> anyhow::Result<ConnectionReport> {
    let mut conn_report = ConnectionReport::new(params.connection_id, params.num_requests);

    let start_instant = Instant::now();

    for n in 0..params.num_requests {
        do_request(&client, &params.target_uri, &mut conn_report, n, &tx_update).await?;
    }

    conn_report.duration = start_instant.elapsed();

    Ok(conn_report)
}

pub async fn do_request(
    client: &impl StatusOnlyHttpClient,
    target_uri: &Uri,
    conn_report: &mut ConnectionReport,
    current_request: u64,
    tx_update: &mpsc::UnboundedSender<BenchmarkUpdate>,
) -> anyhow::Result<()> {
    let start_instant = Instant::now();

    let status_code = client.get(target_uri.clone()).await.context(format!(
        "A request failed (connection #{})",
        conn_report.connection_id
    ))?;
    if status_code < 408 {
        conn_report.ok_requests += 1;
    } else {
        conn_report.failed_requests += 1;
    }

    let duration = Instant::now().duration_since(start_instant);

    conn_report.requests.push(RequestReport {
        status_code,
        duration,
    });

    if (current_request % 100) == 0 {
        let s = BenchmarkUpdate {
            connection_id: conn_report.connection_id,
            current_request,
        };
        tx_update
            .send(s)
            .context("The result channel was closed before the connection was done!")?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use anyhow::{anyhow, Result};
    use async_trait::async_trait;
    use hyper::Uri;
    use tokio::sync::mpsc;

    use crate::{
        cli::{connection_task, ConnectionParameters},
        StatusOnlyHttpClient,
    };

    struct MockHttpClient {
        fixed_status_response: Option<u16>,
    }

    impl MockHttpClient {
        fn with_result(result: Option<u16>) -> Self {
            MockHttpClient {
                fixed_status_response: result,
            }
        }
    }

    #[async_trait]
    impl StatusOnlyHttpClient for MockHttpClient {
        async fn get(&self, _uri: Uri) -> Result<u16> {
            self.fixed_status_response.ok_or(anyhow!("error"))
        }
    }

    #[tokio::test]
    async fn test_happy_path() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let client = MockHttpClient::with_result(Some(200));
        let res = connection_task(client, common_settings(), tx).await;
        let res = res.expect("do not expect a result");
        assert_eq!(res.ok_requests, 10);
        assert_eq!(res.failed_requests, 0);
    }

    #[tokio::test]
    async fn test_with_bad_status_code() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let client = MockHttpClient::with_result(Some(500));
        let res = connection_task(client, common_settings(), tx).await;
        let res = res.expect("do not expect a result");
        assert_eq!(res.ok_requests, 0);
        assert_eq!(res.failed_requests, 10);
    }

    #[tokio::test]
    async fn test_with_error() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let client = MockHttpClient::with_result(None);
        let res = connection_task(client, common_settings(), tx).await;
        res.expect_err("expect a error");
    }

    fn common_settings() -> ConnectionParameters {
        ConnectionParameters {
            connection_id: 0,
            target_uri: Uri::from_static("http://dummy"),
            num_requests: 10,
        }
    }
}
