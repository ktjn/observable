//! PII and secret scrubbing applied during telemetry normalization (`normalize-telemetry`).
//!
//! Ensures prohibited sample data (secrets, tokens, passwords, emails, SSNs,
//! payment cards) is redacted before normalized spans, logs, or metrics are
//! emitted downstream to storage, search, export, sharing, or assistance.

use serde_json::Value;
use std::collections::HashMap;

const REDACTED: &str = "[REDACTED]";
const REDACTED_EMAIL: &str = "[REDACTED_EMAIL]";
const REDACTED_SSN: &str = "[REDACTED_SSN]";
const REDACTED_CARD: &str = "[REDACTED_CARD]";

const SENSITIVE_KEY_SEGMENTS: &[&str] = &[
    "authorization",
    "password",
    "passwd",
    "secret",
    "token",
    "api_key",
    "apikey",
    "private_key",
    "credential",
    "credentials",
    "cookie",
    "ssn",
    "cvv",
    "credit_card",
    "card_number",
    "card",
    "email",
    "phone",
];

const SECRET_PREFIXES: &[&str] = &["sk_live_", "sk_test_", "ghp_", "AKIA", "obs_"];

/// Returns true if an attribute key names a sensitive field.
pub fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    if SENSITIVE_KEY_SEGMENTS.iter().any(|seg| {
        lower == *seg
            || lower.contains("api_key")
            || lower.contains("private_key")
            || lower.contains("credit_card")
            || lower.contains("card_number")
    }) {
        return true;
    }
    lower
        .split(['.', '-', '_', '/'])
        .any(|part| SENSITIVE_KEY_SEGMENTS.contains(&part))
}

/// Scrubs an attribute map in place: sensitive keys have their values replaced
/// with `"[REDACTED]"`, and all remaining string values are scanned for PII/secrets.
pub fn scrub_attributes(attrs: &mut HashMap<String, Value>) {
    for (key, value) in attrs.iter_mut() {
        if is_sensitive_key(key) {
            *value = Value::String(REDACTED.to_owned());
        } else {
            scrub_json_value(value);
        }
    }
}

/// Scrubs a string-valued attribute map (used by `MetricSeries`) in place.
pub fn scrub_string_attributes(attrs: &mut HashMap<String, String>) {
    for (key, value) in attrs.iter_mut() {
        if is_sensitive_key(key) {
            *value = REDACTED.to_owned();
        } else {
            *value = scrub_text(value);
        }
    }
}

/// Recursively scrubs PII and secret patterns inside a `serde_json::Value`.
pub fn scrub_json_value(value: &mut Value) {
    match value {
        Value::String(s) => {
            *s = scrub_text(s);
        }
        Value::Array(items) => {
            for item in items {
                scrub_json_value(item);
            }
        }
        Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                if is_sensitive_key(k) {
                    *v = Value::String(REDACTED.to_owned());
                } else {
                    scrub_json_value(v);
                }
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

/// Scrubs prohibited secret and PII patterns from arbitrary text.
pub fn scrub_text(input: &str) -> String {
    let mut out = scrub_auth_schemes(input);
    out = scrub_prefixed_tokens(&out);
    out = scrub_jwts(&out);
    out = scrub_emails(&out);
    out = scrub_ssns(&out);
    out = scrub_cards(&out);
    out
}

fn scrub_auth_schemes(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut i = 0;
    let bytes = input.as_bytes();
    while i < bytes.len() {
        let rest = &input[i..];
        let scheme = if rest.starts_with("Bearer ") {
            Some("Bearer ")
        } else if rest.starts_with("Basic ") {
            Some("Basic ")
        } else {
            None
        };
        if let Some(prefix) = scheme {
            result.push_str(prefix);
            result.push_str(REDACTED);
            i += prefix.len();
            while i < bytes.len()
                && !bytes[i].is_ascii_whitespace()
                && !matches!(bytes[i], b'"' | b'\'' | b',' | b';' | b')' | b']' | b'}')
            {
                i += 1;
            }
        } else if let Some(ch) = rest.chars().next() {
            result.push(ch);
            i += ch.len_utf8();
        }
    }
    result
}

fn scrub_prefixed_tokens(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut i = 0;
    let bytes = input.as_bytes();
    while i < bytes.len() {
        let rest = &input[i..];
        if let Some(prefix) = SECRET_PREFIXES.iter().find(|p| rest.starts_with(**p)) {
            let mut end = i + prefix.len();
            while end < bytes.len()
                && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'_' | b'-' | b'.'))
            {
                end += 1;
            }
            if end > i + prefix.len() + 3 {
                result.push_str(REDACTED);
                i = end;
                continue;
            }
        }
        if let Some(ch) = rest.chars().next() {
            result.push(ch);
            i += ch.len_utf8();
        }
    }
    result
}

fn scrub_jwts(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut i = 0;
    let bytes = input.as_bytes();
    while i < bytes.len() {
        let rest = &input[i..];
        if rest.starts_with("eyJ") {
            let mut end = i;
            let mut dots = 0;
            while end < bytes.len()
                && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'_' | b'-' | b'.'))
            {
                if bytes[end] == b'.' {
                    dots += 1;
                }
                end += 1;
            }
            if dots >= 1 && end - i >= 16 {
                result.push_str(REDACTED);
                i = end;
                continue;
            }
        }
        if let Some(ch) = rest.chars().next() {
            result.push(ch);
            i += ch.len_utf8();
        }
    }
    result
}

fn is_email_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'%' | b'+' | b'-')
}

fn scrub_emails(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if is_email_char(bytes[i]) {
            let start = i;
            let mut j = i;
            while j < bytes.len() && is_email_char(bytes[j]) {
                j += 1;
            }
            if j > start && j < bytes.len() && bytes[j] == b'@' {
                let mut k = j + 1;
                let mut has_dot = false;
                while k < bytes.len()
                    && (bytes[k].is_ascii_alphanumeric() || matches!(bytes[k], b'.' | b'-'))
                {
                    if bytes[k] == b'.' {
                        has_dot = true;
                    }
                    k += 1;
                }
                if has_dot && k > j + 3 && bytes[k - 1] != b'.' {
                    result.push_str(REDACTED_EMAIL);
                    i = k;
                    continue;
                }
            }
            result.push_str(&input[start..j]);
            i = j;
        } else if let Some(ch) = input[i..].chars().next() {
            result.push(ch);
            i += ch.len_utf8();
        }
    }
    result
}

fn scrub_ssns(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 11 <= bytes.len()
            && bytes[i..i + 3].iter().all(u8::is_ascii_digit)
            && bytes[i + 3] == b'-'
            && bytes[i + 4..i + 6].iter().all(u8::is_ascii_digit)
            && bytes[i + 6] == b'-'
            && bytes[i + 7..i + 11].iter().all(u8::is_ascii_digit)
        {
            let left_ok = i == 0 || !bytes[i - 1].is_ascii_digit();
            let right_ok = i + 11 == bytes.len() || !bytes[i + 11].is_ascii_digit();
            if left_ok && right_ok {
                result.push_str(REDACTED_SSN);
                i += 11;
                continue;
            }
        }
        if let Some(ch) = input[i..].chars().next() {
            result.push(ch);
            i += ch.len_utf8();
        }
    }
    result
}

fn scrub_cards(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() && (i == 0 || !bytes[i - 1].is_ascii_digit()) {
            let mut j = i;
            let mut digits = 0usize;
            while j < bytes.len() {
                if bytes[j].is_ascii_digit() {
                    digits += 1;
                    j += 1;
                } else if matches!(bytes[j], b' ' | b'-')
                    && j + 1 < bytes.len()
                    && bytes[j + 1].is_ascii_digit()
                {
                    j += 1;
                } else {
                    break;
                }
            }
            let right_ok = j == bytes.len() || !bytes[j].is_ascii_digit();
            if (13..=19).contains(&digits) && right_ok {
                result.push_str(REDACTED_CARD);
                i = j;
                continue;
            }
        }
        if let Some(ch) = input[i..].chars().next() {
            result.push(ch);
            i += ch.len_utf8();
        }
    }
    result
}
