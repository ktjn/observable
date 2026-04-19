/// Transport trait — implemented by each transport module.
/// A transport produces raw byte frames and a source address/identifier.
pub mod syslog;
pub mod http_webhook;
pub mod mqtt;
pub mod kafka;
pub mod file_tail;
pub mod stdin;

use anyhow::Result;
use async_trait::async_trait;

pub struct RawFrame {
    pub bytes: Vec<u8>,
    pub source: String,
}

#[async_trait]
pub trait Transport: Send + Sync {
    async fn next_frame(&mut self) -> Result<RawFrame>;
}
