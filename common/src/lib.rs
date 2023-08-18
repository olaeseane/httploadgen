use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;

use hyper::{client::HttpConnector, Body, Client, Uri};

pub mod agent;
pub mod becnhmark;
pub mod cli;

pub use becnhmark::do_request_raw;

pub type HttpClient = Client<HttpConnector, Body>;

#[async_trait]
pub trait StatusOnlyHttpClient {
    async fn get(&self, uri: Uri) -> Result<u16>;
}

#[async_trait]
impl StatusOnlyHttpClient for HttpClient {
    async fn get(&self, uri: Uri) -> Result<u16> {
        Ok(self.get(uri).await?.status().into())
    }
}

#[derive(Debug, Default)]
pub struct RequestReport {
    pub status_code: u16,
    pub duration: Duration,
}
