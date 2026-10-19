//! Source and documentation selection with stable original line numbers.

use regex::RegexBuilder;
use serde::{Deserialize, Serialize};
use std::fmt::Write;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Request {
    pub head: i64,
    pub tail: i64,
    pub start_line: i64,
    pub end_line: i64,
    pub pattern: String,
    pub is_regex: bool,
    pub before: i64,
    pub after: i64,
    pub line_numbers: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Match {
    pub line_number: usize,
    pub line: String,
    pub is_match: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Result {
    pub source: String,
    pub total_lines: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matches: Vec<Match>,
}

fn validate_request(request: &Request) -> std::result::Result<(), String> {
    if [
        request.head,
        request.tail,
        request.start_line,
        request.end_line,
        request.before,
        request.after,
    ]
    .iter()
    .any(|value| *value < 0)
    {
        return Err("selection parameters cannot be negative".into());
    }
    if request.end_line > 0 && request.end_line < request.start_line {
        return Err("end_line must be at least start_line".into());
    }
    Ok(())
}

/// Select the whole file, preserving its original line coordinates.
pub fn apply(text: &str, request: &Request) -> std::result::Result<Result, String> {
    validate_request(request)?;
    let all: Vec<&str> = text.split('\n').collect();
    select(&all, request, 1, all.len())
}

/// Select within a strict 1-based inclusive window of the original text.
/// Request ranges are relative to that window; returned coordinates and
/// total_lines describe the original file. Invalid requests fail before clamping.
pub fn apply_window(
    text: &str,
    request: &Request,
    window_start: usize,
    window_end: usize,
) -> std::result::Result<Result, String> {
    validate_request(request)?;
    let all: Vec<&str> = text.split('\n').collect();
    if window_start == 0 || window_end < window_start || window_end > all.len() {
        return Err("window must be a valid 1-based inclusive range".into());
    }
    select(
        &all[window_start - 1..window_end],
        request,
        window_start,
        all.len(),
    )
}

fn select(
    all: &[&str],
    request: &Request,
    origin: usize,
    total: usize,
) -> std::result::Result<Result, String> {
    let mut lines = all;
    let mut offset = origin;
    let mut result = Result {
        source: String::new(),
        total_lines: total,
        start_line: None,
        end_line: None,
        matches: Vec::new(),
    };
    if request.start_line > 0 || request.end_line > 0 {
        let start = request.start_line.max(1).min(all.len() as i64) as usize;
        let end = if request.end_line <= 0 {
            all.len()
        } else {
            (request.end_line as usize).min(all.len())
        }
        .max(start);
        lines = &all[start - 1..end];
        offset = origin + start - 1;
        result.start_line = Some(offset);
        result.end_line = Some(origin + end - 1);
    }
    if request.head > 0 && (request.head as usize) < lines.len() {
        lines = &lines[..request.head as usize];
        result.start_line = Some(offset);
        result.end_line = Some(offset + lines.len() - 1);
    }
    if request.tail > 0 && (request.tail as usize) < lines.len() {
        let start = lines.len() - request.tail as usize;
        offset += start;
        lines = &lines[start..];
        result.start_line = Some(offset);
        result.end_line = Some(offset + lines.len() - 1);
    }
    if !request.pattern.is_empty() {
        result.matches = search(lines, offset, request)?;
        result.source = format_matches(&result.matches);
        return Ok(result);
    }
    result.source = if request.line_numbers {
        let mut formatted = String::new();
        for (index, line) in lines.iter().enumerate() {
            writeln!(formatted, "{:4}: {line}", offset + index)
                .expect("writing to a String cannot fail");
        }
        formatted.trim_end_matches('\n').to_owned()
    } else {
        lines.join("\n")
    };
    if result.start_line.is_none() {
        result.start_line = Some(offset);
        result.end_line = Some(offset + lines.len() - 1);
    }
    Ok(result)
}

fn search(
    lines: &[&str],
    offset: usize,
    request: &Request,
) -> std::result::Result<Vec<Match>, String> {
    let pattern = if request.is_regex {
        request.pattern.clone()
    } else {
        regex::escape(&request.pattern)
    };
    let expression = RegexBuilder::new(&pattern)
        .case_insensitive(!request.is_regex)
        .build()
        .map_err(|error| error.to_string())?;
    let hits: Vec<bool> = lines.iter().map(|line| expression.is_match(line)).collect();
    let mut included = vec![false; lines.len()];
    let mut covered_end = 0;
    for (index, hit) in hits.iter().enumerate() {
        if !hit {
            continue;
        }
        let start = index.saturating_sub(request.before as usize);
        let end = index
            .saturating_add(request.after as usize)
            .min(lines.len() - 1);
        if start <= end {
            let new_start = start.max(covered_end);
            let new_end = end + 1;
            if new_start < new_end {
                included[new_start..new_end].fill(true);
                covered_end = new_end;
            }
        }
    }
    Ok(included
        .iter()
        .enumerate()
        .filter(|(_, include)| **include)
        .map(|(index, _)| Match {
            line_number: offset + index,
            line: lines[index].to_owned(),
            is_match: hits[index],
        })
        .collect())
}

fn format_matches(matches: &[Match]) -> String {
    let mut formatted = String::new();
    let mut previous = None;
    for hit in matches {
        if previous.is_some_and(|line| hit.line_number > line + 1) {
            formatted.push_str("---\n");
        }
        writeln!(
            formatted,
            "{} {:4}: {}",
            if hit.is_match { '>' } else { ' ' },
            hit.line_number,
            hit.line
        )
        .expect("writing to a String cannot fail");
        previous = Some(hit.line_number);
    }
    formatted.trim_end_matches('\n').to_owned()
}
