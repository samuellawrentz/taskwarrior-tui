//! Compact task detail pane: header + annotations as a notes area.
use std::sync::LazyLock;

use chrono::{Local, TimeZone};
use regex::Regex;
use task_hookrs::{task::Task, uda::UDAValue};
use unicode_width::UnicodeWidthStr;

use crate::task_report::vague_format_date_time;

pub static URL_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"https?://[^\s<>"'\)\]]+"#).unwrap());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
  Header,
  Title,
  Section,
  Note,
  /// `Label value · Label value`; labels drawn dim
  Props,
}

/// Word-wrap; a word longer than `width` gets its own line untouched so URLs stay intact.
pub fn wrap(text: &str, width: usize, indent: &str) -> Vec<String> {
  let width = width.max(8);
  let mut lines = vec![];
  let (mut cur, mut cur_w, mut prefix): (Vec<&str>, usize, &str) = (vec![], 0, "");
  for word in text.split_whitespace() {
    if !cur.is_empty() && cur_w + 1 + word.width() > width {
      lines.push(format!("{}{}", prefix, cur.join(" ")));
      cur.clear();
      prefix = indent;
      cur_w = indent.width();
    }
    cur_w += word.width() + usize::from(!cur.is_empty());
    cur.push(word);
  }
  if !cur.is_empty() {
    lines.push(format!("{}{}", prefix, cur.join(" ")));
  }
  lines
}

pub fn render(task: &Task, width: usize, virtual_tags: &[String]) -> Vec<(String, Kind)> {
  let mut out = vec![];
  let updated = task
    .modified()
    .map(|m| format!("updated {} ago", vague_format_date_time(**m, Local::now().naive_utc(), false)))
    .unwrap_or_default();
  let status = format!("{:?}", task.status()).to_lowercase();
  let project = task.project().cloned().unwrap_or_default();
  out.push((
    format!("#{}  {}  {}  {}", task.id().unwrap_or_default(), status, project, updated),
    Kind::Header,
  ));
  for l in wrap(task.description(), width, "") {
    out.push((l, Kind::Title));
  }
  let uda = |k: &str| match task.uda().get(k) {
    Some(UDAValue::Str(v)) => v.clone(),
    Some(UDAValue::U64(v)) => v.to_string(),
    Some(UDAValue::F64(v)) => v.to_string(),
    None => "-".into(),
  };
  let mut props = vec![format!("Owner {}", uda("owner")), format!("From {}", uda("from"))];
  if let Some(d) = task.due() {
    props.push(format!("Due {}", vague_format_date_time(Local::now().naive_utc(), **d, false)));
  }
  // the TUI appends virtual tags (PENDING, UNBLOCKED, ...) to tags in memory
  let tags: Vec<&str> = task
    .tags()
    .map(|t| t.iter().filter(|t| !virtual_tags.contains(t)).map(String::as_str).collect())
    .unwrap_or_default();
  if !tags.is_empty() {
    props.push(format!("Tags {}", tags.join(" ")));
  }
  out.push((String::new(), Kind::Note));
  out.push((props.join(" · "), Kind::Props));
  out.push((String::new(), Kind::Note));
  let notes = task.annotations().map(|a| a.as_slice()).unwrap_or(&[]);
  out.push((format!("Notes ({})", notes.len()), Kind::Section));
  for a in notes {
    let ts = Local.from_utc_datetime(a.entry()).format("%Y-%m-%d %H:%M").to_string();
    for l in wrap(&format!("{}  {}", ts, a.description()), width, "  ") {
      out.push((l, Kind::Note));
    }
  }
  out
}

pub fn url_at(line: &str, col: usize) -> Option<String> {
  URL_RE.find_iter(line).find(|m| {
    let start = line[..m.start()].width();
    let end = start + m.as_str().width();
    (start..end).contains(&col)
  }).map(|m| m.as_str().to_string())
}

pub fn open_url(url: &str) {
  let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
  let _ = std::process::Command::new(opener).arg(url).spawn();
}

fn task_run(task_exe: &str, uuid: &str, verb: &str, text: &str) -> Result<(), String> {
  let o = std::process::Command::new(task_exe)
    .args(["rc.bulk=0", "rc.confirmation=off", "rc.verbose=nothing", uuid, verb, "--", text])
    .output()
    .map_err(|e| format!("cannot run task {}: {}", verb, e))?;
  if o.status.success() {
    Ok(())
  } else {
    Err(format!("task {} failed: {}", verb, String::from_utf8_lossy(&o.stderr)))
  }
}

/// Open the notes in $VISUAL/$EDITOR, one per line. Removed lines are denotated, new lines annotated.
/// Caller must pause the TUI first.
pub fn edit(task_exe: &str, task: &Task) -> Result<(), String> {
  let uuid = task.uuid().to_string();
  let old: Vec<String> = task
    .annotations()
    .map(|a| a.iter().map(|x| x.description().clone()).collect())
    .unwrap_or_default();
  let path = std::env::temp_dir().join(format!("twt-notes-{}.md", uuid));
  let body = format!(
    "# {}\n# one note per line; delete a line to remove it; lines starting with # are ignored\n{}\n",
    task.description(),
    old.join("\n")
  );
  std::fs::write(&path, body).map_err(|e| e.to_string())?;
  let editor = std::env::var("VISUAL").or_else(|_| std::env::var("EDITOR")).unwrap_or_else(|_| "vi".into());
  let parts = shlex::split(&editor).unwrap_or_else(|| vec![editor.clone()]);
  let status = std::process::Command::new(&parts[0])
    .args(&parts[1..])
    .arg(&path)
    .status()
    .map_err(|e| format!("cannot run {}: {}", editor, e))?;
  if !status.success() {
    return Ok(());
  }
  let new: Vec<String> = std::fs::read_to_string(&path)
    .map_err(|e| e.to_string())?
    .lines()
    .map(str::trim)
    .filter(|l| !l.is_empty() && !l.starts_with('#'))
    .map(String::from)
    .collect();
  let _ = std::fs::remove_file(&path);
  for n in old.iter().filter(|n| !new.contains(n)) {
    task_run(task_exe, &uuid, "denotate", n)?;
  }
  for n in new.iter().filter(|n| !old.contains(n)) {
    task_run(task_exe, &uuid, "annotate", n)?;
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn wrap_keeps_long_urls_whole() {
    let url = "https://plivo.slack.com/archives/C0AFJE74LTX/p1774420819875539";
    let lines = wrap(&format!("see {} later", url), 20, "  ");
    assert_eq!(lines, vec!["see", &format!("  {}", url), "  later"]);
    assert_eq!(url_at(&lines[1], 5).as_deref(), Some(url));
    assert_eq!(url_at(&lines[1], 0), None);
  }
}
