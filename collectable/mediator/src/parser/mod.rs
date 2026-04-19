/// Parser trait — implemented by each parser module.
/// A parser consumes a raw frame and produces structured fields.
pub mod json;
pub mod grok;
pub mod key_value;
pub mod multiline;
pub mod log4j2_pattern;
pub mod log4j2_json;
pub mod regex;
pub mod csv;
pub mod passthrough;

use anyhow::Result;
use std::collections::HashMap;

pub type Fields = HashMap<String, serde_json::Value>;

pub trait Parser: Send + Sync {
    fn parse(&self, raw: &[u8]) -> Result<Fields>;
}
