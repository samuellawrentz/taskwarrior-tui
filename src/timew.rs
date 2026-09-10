//! Read timewarrior intervals to show time worked per task.
use std::{collections::HashMap, process::Command};

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
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

fn export() -> String {
  Command::new("timew")
    .arg("export")
    .output()
    .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
    .unwrap_or_default()
}

fn is_uuid(t: &str) -> bool {
  t.len() == 36 && t.bytes().all(|b| b == b'-' || b.is_ascii_hexdigit())
}

/// Seconds tracked per timewarrior tag. The open interval counts up to now.
/// Intervals are matched to tasks by the task uuid tag, which the on-modify hook must add.
pub fn worked_seconds() -> HashMap<String, i64> {
  parse(&export(), Utc::now().naive_utc())
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

/// This week's intervals grouped by local day, then by task (tags minus the uuid), newest day first.
pub fn week_report() -> String {
  week_report_from(&export(), Local::now().naive_utc(), Local::now().date_naive())
}

fn week_report_from(json: &str, now_utc: NaiveDateTime, today: NaiveDate) -> String {
  let intervals: Vec<Interval> = serde_json::from_str(json).unwrap_or_default();
  let monday = today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64);
  // day -> task label -> (secs, running)
  let mut days: std::collections::BTreeMap<NaiveDate, std::collections::BTreeMap<String, (i64, bool)>> = Default::default();
  for i in intervals {
    let Ok(start) = NaiveDateTime::parse_from_str(&i.start, FMT) else {
      continue;
    };
    let end = i.end.as_deref().and_then(|e| NaiveDateTime::parse_from_str(e, FMT).ok());
    let day = Local.from_utc_datetime(&start).date_naive();
    if day < monday {
      continue;
    }
    let secs = (end.unwrap_or(now_utc) - start).num_seconds();
    let label = i.tags.iter().filter(|t| !is_uuid(t)).cloned().collect::<Vec<_>>().join(", ");
    let e = days.entry(day).or_default().entry(label).or_insert((0, false));
    e.0 += secs;
    e.1 |= end.is_none();
  }
  let week_total: i64 = days.values().flat_map(|d| d.values().map(|v| v.0)).sum();
  let mut out = vec![format!(
    "Wk {}  {} .. {}{:>40}",
    monday.iso_week().week(),
    monday,
    monday + chrono::Duration::days(6),
    format_hm(week_total)
  )];
  for (day, tasks) in days.iter().rev() {
    let day_total: i64 = tasks.values().map(|v| v.0).sum();
    out.push(String::new());
    out.push(format!("{}{:>52}", day.format("%a %Y-%m-%d"), format_hm(day_total)));
    let mut rows: Vec<_> = tasks.iter().collect();
    rows.sort_by_key(|(_, v)| -v.0);
    for (label, (secs, running)) in rows {
      out.push(format!(
        "  {:<50}{:>10}{}",
        label,
        format_hm(*secs),
        if *running { "  running" } else { "" }
      ));
    }
  }
  out.join("\n")
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

  #[test]
  fn week_report_groups_by_day_and_task_and_drops_uuid() {
    let json = r#"[
      {"id":3,"start":"20260901T100000Z","end":"20260901T110000Z","tags":["old"]},
      {"id":2,"start":"20260910T100000Z","end":"20260910T101500Z","tags":["286f8c33-39be-4dff-9aae-099c7bf945fd","comms","review theme"]},
      {"id":1,"start":"20260911T120000Z","tags":["286f8c33-39be-4dff-9aae-099c7bf945fd","comms","review theme"]}
    ]"#;
    let now = NaiveDateTime::parse_from_str("20260911T120500Z", FMT).unwrap();
    let r = week_report_from(json, now, NaiveDate::from_ymd_opt(2026, 9, 11).unwrap());
    assert!(r.starts_with("Wk 37  2026-09-07 .. 2026-09-13"));
    assert!(!r.contains("286f8c33"));
    assert!(r.contains("comms, review theme"));
    assert!(r.contains("running"));
    assert!(!r.contains("old"));
  }
}
