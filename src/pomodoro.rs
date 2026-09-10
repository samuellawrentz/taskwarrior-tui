//! Pomodoro timer backed by taskwarrior start/stop (timewarrior logs the interval via its hook).
use std::{
  process::Command,
  time::{Duration, Instant},
};

use uuid::Uuid;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
  Idle,
  Work,
  Break,
}

pub struct Pomodoro {
  pub phase: Phase,
  pub task: Option<(Uuid, String)>,
  pub started: Instant,
  pub length: Duration,
  pub completed: u32,
  pub work: Duration,
  pub rest: Duration,
  pub sound: String,
}

impl Pomodoro {
  pub fn new(work_min: u64, break_min: u64, sound: String) -> Self {
    Self {
      phase: Phase::Idle,
      task: None,
      started: Instant::now(),
      length: Duration::ZERO,
      completed: 0,
      work: Duration::from_secs(work_min * 60),
      rest: Duration::from_secs(break_min * 60),
      sound,
    }
  }

  pub fn remaining(&self) -> Duration {
    match self.phase {
      Phase::Idle => self.work,
      _ => self.length.saturating_sub(self.started.elapsed()),
    }
  }

  pub fn progress(&self) -> f64 {
    if self.phase == Phase::Idle || self.length.is_zero() {
      return 0.0;
    }
    1.0 - self.remaining().as_secs_f64() / self.length.as_secs_f64()
  }

  /// Start a work session on `task` (runs `task <uuid> start`). Stops any running session first.
  pub fn start(&mut self, task_exe: &str, uuid: Uuid, description: String) {
    self.stop(task_exe);
    run(task_exe, &[&uuid.to_string(), "start"]);
    self.task = Some((uuid, description));
    self.begin(Phase::Work, self.work);
  }

  pub fn start_break(&mut self, task_exe: &str) {
    self.stop(task_exe);
    self.begin(Phase::Break, self.rest);
  }

  /// Stop whatever is running. A work session runs `task <uuid> stop` so timewarrior records it.
  pub fn stop(&mut self, task_exe: &str) {
    if self.phase == Phase::Work
      && let Some((uuid, _)) = &self.task
    {
      run(task_exe, &[&uuid.to_string(), "stop"]);
    }
    self.phase = Phase::Idle;
  }

  /// Call on every tick. Work -> Break -> Idle, with a sound + notification at each boundary.
  pub fn tick(&mut self, task_exe: &str) {
    if self.phase == Phase::Idle || !self.remaining().is_zero() {
      return;
    }
    match self.phase {
      Phase::Work => {
        self.completed += 1;
        self.notify("Pomodoro done. Take a break.");
        self.start_break(task_exe);
      }
      Phase::Break => {
        self.notify("Break over.");
        self.phase = Phase::Idle;
      }
      Phase::Idle => {}
    }
  }

  fn begin(&mut self, phase: Phase, length: Duration) {
    self.phase = phase;
    self.length = length;
    self.started = Instant::now();
  }

  // ponytail: macOS only (afplay + osascript). Add a Linux branch (paplay/notify-send) if ever needed.
  fn notify(&self, msg: &str) {
    let _ = Command::new("afplay").arg(&self.sound).spawn();
    let script = format!(r#"display notification "{}" with title "taskwarrior-tui""#, msg);
    let _ = Command::new("osascript").arg("-e").arg(script).spawn();
  }
}

fn run(task_exe: &str, args: &[&str]) {
  let _ = Command::new(task_exe).args(args).output();
}

/// 3x5 glyphs for 0-9 and ':'; each cell doubles to two block chars on screen.
const FONT: [[&str; 5]; 11] = [
  ["###", "# #", "# #", "# #", "###"],
  [" # ", "## ", " # ", " # ", "###"],
  ["###", "  #", "###", "#  ", "###"],
  ["###", "  #", "###", "  #", "###"],
  ["# #", "# #", "###", "  #", "  #"],
  ["###", "#  ", "###", "  #", "###"],
  ["###", "#  ", "###", "# #", "###"],
  ["###", "  #", "  #", "  #", "  #"],
  ["###", "# #", "###", "# #", "###"],
  ["###", "# #", "###", "  #", "###"],
  ["   ", " # ", "   ", " # ", "   "],
];

pub fn big_clock(d: Duration) -> Vec<String> {
  let secs = d.as_secs();
  let text = format!("{:02}:{:02}", secs / 60, secs % 60);
  (0..5)
    .map(|row| {
      text
        .chars()
        .map(|c| {
          let g = if c == ':' { 10 } else { c.to_digit(10).unwrap() as usize };
          FONT[g][row].chars().map(|p| if p == '#' { "██" } else { "  " }).collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("  ")
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn big_clock_renders_five_rows_of_equal_width() {
    let rows = big_clock(Duration::from_secs(25 * 60));
    assert_eq!(rows.len(), 5);
    assert!(rows.iter().all(|r| r.chars().count() == rows[0].chars().count()));
    assert_eq!(rows[0], "██████  ██████          ██████  ██████");
  }

  #[test]
  fn idle_shows_full_work_length_and_no_progress() {
    let p = Pomodoro::new(25, 5, String::new());
    assert_eq!(p.remaining(), Duration::from_secs(1500));
    assert_eq!(p.progress(), 0.0);
  }
}
