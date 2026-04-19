/// Parser preview engine — runs the real parser logic against sample lines.
///
/// Each variant mirrors the parser types available in the UI. The results are
/// returned as a list of field→value maps so the UI can render a live table.
use regex::Regex;
use std::collections::HashMap;

pub type ParsedRow = HashMap<String, String>;

/// Parse up to `lines.len()` sample lines using the given parser type and params.
pub fn parse_lines(
    parser_type: &str,
    params: &HashMap<String, serde_json::Value>,
    lines: &[String],
) -> Result<Vec<ParsedRow>, String> {
    let lines: Vec<&str> = lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    if lines.is_empty() {
        return Ok(vec![]);
    }

    match parser_type {
        "json" | "log4j2_json" => parse_json(&lines),
        "regex" => parse_regex(params, &lines),
        "grok" => parse_grok(params, &lines),
        "key_value" => parse_key_value(params, &lines),
        "csv" => parse_csv(params, &lines),
        "log4j2_pattern" => parse_log4j2_pattern(params, &lines),
        "passthrough" => Ok(lines
            .iter()
            .map(|l| [("$raw".to_string(), l.to_string())].into())
            .collect()),
        "multiline" => Ok(lines
            .iter()
            .map(|l| [("$raw".to_string(), l.to_string())].into())
            .collect()),
        _ => Err(format!("Unknown parser type: {parser_type}")),
    }
}

// ── JSON ─────────────────────────────────────────────────────────────────────

fn parse_json(lines: &[&str]) -> Result<Vec<ParsedRow>, String> {
    lines
        .iter()
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .map_err(|e| format!("JSON parse error on line {:?}: {e}", &line[..line.len().min(60)]))
                .and_then(|v| match v {
                    serde_json::Value::Object(map) => Ok(map
                        .into_iter()
                        .map(|(k, v)| {
                            let s = match v {
                                serde_json::Value::String(s) => s,
                                other => other.to_string(),
                            };
                            (k, s)
                        })
                        .collect()),
                    _ => Err(format!("Line is not a JSON object: {:?}", &line[..line.len().min(60)])),
                })
        })
        .collect()
}

// ── Regex ─────────────────────────────────────────────────────────────────────

fn apply_regex(re: &Regex, lines: &[&str]) -> Vec<ParsedRow> {
    lines
        .iter()
        .map(|line| match re.captures(line) {
            Some(caps) => re
                .capture_names()
                .flatten()
                .map(|name| {
                    (
                        name.to_string(),
                        caps.name(name).map(|m| m.as_str().to_string()).unwrap_or_default(),
                    )
                })
                .collect(),
            None => [("⚠".to_string(), format!("no match: {}", &line[..line.len().min(60)]))].into(),
        })
        .collect()
}

fn parse_regex(params: &HashMap<String, serde_json::Value>, lines: &[&str]) -> Result<Vec<ParsedRow>, String> {
    let pattern = params
        .get("pattern")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'pattern' parameter")?;
    let re = Regex::new(pattern).map_err(|e| format!("Regex compile error: {e}"))?;
    Ok(apply_regex(&re, lines))
}

// ── Grok ──────────────────────────────────────────────────────────────────────

fn grok_builtin(name: &str) -> Option<&'static str> {
    Some(match name {
        "TIMESTAMP_ISO8601" => r"\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:[.,]\d+)?(?:Z|[+-]\d{2}:?\d{2})?",
        "DATESTAMP" => r"\d{1,2}/\w+/\d{4}:\d{2}:\d{2}:\d{2} [+-]\d{4}",
        "LOGLEVEL" | "LOGLEVEL_SYSLOG" => {
            r"(?:DEBUG|INFO|WARN(?:ING)?|ERROR|FATAL|TRACE|CRITICAL|EMERG|ALERT|CRIT|NOTICE)"
        }
        "GREEDYDATA" => r".*",
        "DATA" => r".*?",
        "WORD" => r"\w+",
        "NOTSPACE" => r"\S+",
        "SPACE" => r"\s*",
        "NUMBER" => r"\d+(?:\.\d+)?",
        "INT" | "POSINT" | "NONNEGINT" => r"\d+",
        "FLOAT" => r"\d+\.\d+",
        "IP" | "IPV4" => r"\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}",
        "HOSTNAME" | "HOST" => r"[a-zA-Z0-9._-]+",
        "UUID" => r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}",
        "BASE16NUM" => r"(?:0x)?[0-9a-fA-F]+",
        "QUOTEDSTRING" | "QS" => r#""(?:[^"\\]|\\.)*""#,
        "PATH" => r"(?:/[\w.@-]*)+",
        "USERNAME" | "USER" => r"[a-zA-Z0-9._-]+",
        _ => return None,
    })
}

fn grok_to_regex(grok_pattern: &str) -> Result<String, String> {
    let mut out = String::from("^");
    let mut chars = grok_pattern.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            let mut token = String::new();
            for ch in chars.by_ref() {
                if ch == '}' {
                    break;
                }
                token.push(ch);
            }
            let (pat_name, field_name) = match token.find(':') {
                Some(pos) => (&token[..pos], Some(&token[pos + 1..])),
                None => (token.as_str(), None),
            };
            let pat_re = grok_builtin(pat_name)
                .ok_or_else(|| format!("Unknown Grok pattern: {pat_name}"))?;
            match field_name {
                Some(fname) => out.push_str(&format!("(?P<{fname}>{pat_re})")),
                None => out.push_str(pat_re),
            }
        } else {
            if r"[]()*+?^$|\{}.".contains(c) && c != ' ' {
                out.push('\\');
            }
            out.push(c);
        }
    }
    out.push('$');
    Ok(out)
}

fn parse_grok(params: &HashMap<String, serde_json::Value>, lines: &[&str]) -> Result<Vec<ParsedRow>, String> {
    let pattern = params
        .get("pattern")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'pattern' parameter")?;
    let regex_str = grok_to_regex(pattern)?;
    let re = Regex::new(&regex_str).map_err(|e| format!("Grok→Regex compile error: {e}"))?;
    Ok(apply_regex(&re, lines))
}

// ── Key=Value ─────────────────────────────────────────────────────────────────

fn parse_key_value(params: &HashMap<String, serde_json::Value>, lines: &[&str]) -> Result<Vec<ParsedRow>, String> {
    let sep = params.get("separator").and_then(|v| v.as_str()).unwrap_or(" ");
    Ok(lines
        .iter()
        .map(|line| {
            let mut row = ParsedRow::new();
            for pair in line.split(sep) {
                if let Some(pos) = pair.find('=') {
                    let key = pair[..pos].trim().to_string();
                    let val = pair[pos + 1..].trim().trim_matches('"').to_string();
                    if !key.is_empty() {
                        row.insert(key, val);
                    }
                }
            }
            if row.is_empty() {
                row.insert("⚠".to_string(), "no key=value pairs found".to_string());
            }
            row
        })
        .collect())
}

// ── CSV ───────────────────────────────────────────────────────────────────────

fn parse_csv(params: &HashMap<String, serde_json::Value>, lines: &[&str]) -> Result<Vec<ParsedRow>, String> {
    let delim = params
        .get("delimiter")
        .and_then(|v| v.as_str())
        .and_then(|s| s.chars().next())
        .unwrap_or(',');

    if lines.is_empty() {
        return Ok(vec![]);
    }
    let headers: Vec<String> = lines[0]
        .split(delim)
        .map(|h| h.trim().trim_matches('"').to_string())
        .collect();

    Ok(lines[1..]
        .iter()
        .map(|line| {
            let vals: Vec<&str> = line.split(delim).collect();
            headers
                .iter()
                .enumerate()
                .map(|(i, h)| {
                    (
                        h.clone(),
                        vals.get(i)
                            .map(|v| v.trim().trim_matches('"').to_string())
                            .unwrap_or_default(),
                    )
                })
                .collect()
        })
        .collect())
}

// ── Log4j2 PatternLayout ─────────────────────────────────────────────────────

/// Convert a Log4j2 PatternLayout string into a named-group regex.
fn log4j2_pattern_to_regex(pattern: &str) -> Result<String, String> {
    let mut out = String::from("^");
    let mut seen: Vec<String> = Vec::new();
    let mut chars = pattern.chars().peekable();

    while let Some(c) = chars.next() {
        if c != '%' {
            // Flexible whitespace; escape other regex metacharacters in literals
            if c == ' ' {
                out.push_str(r"\s+");
            } else if r"[]()*+?^$|\".contains(c) {
                out.push('\\');
                out.push(c);
            } else {
                out.push(c);
            }
            continue;
        }

        // Handle '%%' → literal '%'
        if chars.peek() == Some(&'%') {
            chars.next();
            out.push('%');
            continue;
        }

        // Skip optional format modifier: [-]?[0-9]*(\.[0-9]+)?
        while chars.peek().map(|c| c.is_ascii_digit() || *c == '-' || *c == '.').unwrap_or(false) {
            chars.next();
        }

        // Read specifier name (letters only)
        let mut spec = String::new();
        while chars.peek().map(|c| c.is_alphabetic()).unwrap_or(false) {
            spec.push(chars.next().unwrap());
        }

        // Skip optional {argument} block
        if chars.peek() == Some(&'{') {
            chars.next();
            for ch in chars.by_ref() {
                if ch == '}' {
                    break;
                }
            }
        }

        let (field, re): (&str, &str) = match spec.as_str() {
            "d" | "date" => ("timestamp", r"\S+(?:\s\S+)?"),
            "p" | "level" | "le" => ("level", r"\w+"),
            "m" | "msg" | "message" => ("message", r".+"),
            "t" | "thread" | "threadName" => ("thread", r"\S+"),
            "c" | "logger" | "lo" => ("logger", r"[\w.$]+"),
            "L" | "line" => ("line", r"\d+"),
            "C" | "class" => ("class", r"[\w.$]+"),
            "M" | "method" => ("method", r"[\w$]+"),
            "F" | "file" => ("file", r"\S+"),
            "ex" | "exception" | "throwable" | "rEx" => ("exception", r".*"),
            "r" | "relative" => ("elapsed_ms", r"\d+"),
            "n" | "x" | "X" | "NDC" | "mdc" => continue,
            _ => {
                out.push_str(r"\S*");
                continue;
            }
        };

        // Deduplicate — regex crate rejects duplicate named groups
        if seen.contains(&field.to_string()) {
            out.push_str(re);
        } else {
            seen.push(field.to_string());
            out.push_str(&format!("(?P<{field}>{re})"));
        }
    }

    Ok(out)
}

fn parse_log4j2_pattern(
    params: &HashMap<String, serde_json::Value>,
    lines: &[&str],
) -> Result<Vec<ParsedRow>, String> {
    let pattern = params
        .get("pattern")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'pattern' parameter")?;
    let regex_str = log4j2_pattern_to_regex(pattern)?;
    let re = Regex::new(&regex_str).map_err(|e| format!("PatternLayout→Regex error: {e}\nGenerated: {regex_str}"))?;
    Ok(apply_regex(&re, lines))
}
