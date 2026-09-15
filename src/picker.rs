//! Linear-style attribute picker: one key opens a filterable list, Enter runs `task modify`.
//!
//! Config: `uda.taskwarrior-tui.picker.<key>=<attr>[:<v1>,<v2>,...]`
//! No value list → values are pulled live from taskwarrior (`_unique <attr>`, `_projects`, `_tags`).
use std::process::Command;

pub struct Picker {
  pub attr: String,
  pub items: Vec<String>,
  pub search: String,
  pub selected: usize,
}

impl Picker {
  pub fn open(task_exe: &str, spec: &str) -> Self {
    let (attr, values) = match spec.split_once(':') {
      Some((a, v)) => (a.trim(), Some(v)),
      None => (spec.trim(), None),
    };
    let mut items: Vec<String> = match values {
      Some(v) => v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
      None => live_values(task_exe, attr),
    };
    if attr != "tags" {
      items.push("none".to_string());
    }
    Self {
      attr: attr.to_string(),
      items,
      search: String::new(),
      selected: 0,
    }
  }

  pub fn filtered(&self) -> Vec<&str> {
    let q = self.search.to_lowercase();
    self.items.iter().map(String::as_str).filter(|i| i.to_lowercase().contains(&q)).collect()
  }

  /// Highlighted item, or the typed text when nothing matches (new owner name, custom date).
  pub fn value(&self) -> String {
    self
      .filtered()
      .get(self.selected)
      .map(|s| s.to_string())
      .unwrap_or_else(|| self.search.trim().to_string())
  }

  pub fn next(&mut self) {
    let n = self.filtered().len();
    if n > 0 {
      self.selected = (self.selected + 1) % n;
    }
  }

  pub fn prev(&mut self) {
    let n = self.filtered().len();
    if n > 0 {
      self.selected = (self.selected + n - 1) % n;
    }
  }

  /// `task modify` argument. Tags toggle; `none` clears the attribute.
  pub fn modify_arg(&self, value: &str, has_tag: bool) -> String {
    match (self.attr.as_str(), value) {
      ("tags", v) if has_tag => format!("-{v}"),
      ("tags", v) => format!("+{v}"),
      (a, "none") => format!("{a}:"),
      (a, v) => format!("{a}:{v}"),
    }
  }
}

fn live_values(task_exe: &str, attr: &str) -> Vec<String> {
  let cmd = match attr {
    "project" => vec!["_projects"],
    "tags" => vec!["_tags"],
    a => vec!["_unique", a],
  };
  let out = Command::new(task_exe)
    .args(["rc.context=none", "rc.verbose=nothing", "status:pending"])
    .args(cmd)
    .output();
  let Ok(out) = out else { return vec![] };
  String::from_utf8_lossy(&out.stdout)
    .lines()
    .map(str::trim)
    // `_tags` also lists virtual tags (READY, OVERDUE, ...): all-uppercase, skip them
    .filter(|l| !l.is_empty() && l.chars().any(|c| c.is_lowercase()))
    .map(str::to_string)
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  fn picker(attr: &str, items: &[&str]) -> Picker {
    Picker {
      attr: attr.into(),
      items: items.iter().map(|s| s.to_string()).collect(),
      search: String::new(),
      selected: 0,
    }
  }

  #[test]
  fn filter_value_and_args() {
    let mut p = picker("owner", &["akil", "vijay", "vijayk", "none"]);
    p.search = "VIJ".into();
    assert_eq!(p.filtered(), vec!["vijay", "vijayk"]);
    p.next();
    assert_eq!(p.value(), "vijayk");
    p.next();
    assert_eq!(p.value(), "vijay");
    assert_eq!(p.modify_arg("vijay", false), "owner:vijay");
    assert_eq!(p.modify_arg("none", false), "owner:");
    p.search = "ravi".into();
    assert_eq!(p.value(), "ravi");

    let t = picker("tags", &["comms", "review"]);
    assert_eq!(t.modify_arg("comms", false), "+comms");
    assert_eq!(t.modify_arg("comms", true), "-comms");
  }
}
