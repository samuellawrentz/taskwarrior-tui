//! Read timewarrior intervals to show time worked per task.
use std::{collections::HashMap, process::Command};

use chrono::{NaiveDateTime, Utc};
use serde::Deserialize;

#[derive(Deserialize)]
struct Interval {
  start: String,
  #[serde(default)]
  end: Option<String>,
  #[serde(default)]
  tags: Vec<String>,
}

const FMT: &str = "%Y%m%dT%H%M%SZ";

/// Seconds tracked per timewarrior tag. The open interval counts up to now.
/// Intervals are matched to tasks by the task uuid tag, which the on-modify hook must add.
pub fn worked_seconds() -> HashMap<String, i64> {
  match Command::new("timew").arg("export").output() {
    Ok(out) => parse(&String::from_utf8_lossy(&out.stdout), Utc::now().naive_utc()),
    Err(_) => HashMap::new(),
  }
}

fn parse(json: &str, now: NaiveDateTime) -> HashMap<String, i64> {
  let intervals: Vec<Interval> = serde_json::from_str(json).unwrap_or_default();
  let mut m = HashMap::new();
  for i in intervals {
    let Ok(start) = NaiveDateTime::parse_from_str(&i.start, FMT) else {
      continue;
    };
    let end = i.end.as_deref().and_then(|e| NaiveDateTime::parse_from_str(e, FMT).ok()).unwrap_or(now);
    let secs = (end - start).num_seconds();
    for t in i.tags {
      *m.entry(t).or_insert(0) += secs;
    }
  }
  m
}

pub fn format_hm(secs: i64) -> String {
  if secs <= 0 {
    return String::new();
  }
  let (h, m) = (secs / 3600, secs % 3600 / 60);
  if h > 0 { format!("{h}h{m:02}m") } else { format!("{m}m") }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn sums_closed_and_open_intervals_per_tag() {
    let json = r#"[
      {"id":2,"start":"20260910T100000Z","end":"20260910T101500Z","tags":["u1","desc"]},
      {"id":1,"start":"20260910T120000Z","tags":["u1"]}
    ]"#;
    let now = NaiveDateTime::parse_from_str("20260910T120500Z", FMT).unwrap();
    let m = parse(json, now);
    assert_eq!(m["u1"], 20 * 60);
    assert_eq!(m["desc"], 15 * 60);
    assert_eq!(format_hm(m["u1"]), "20m");
    assert_eq!(format_hm(3660), "1h01m");
    assert_eq!(format_hm(0), "");
  }
}
