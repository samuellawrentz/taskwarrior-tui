//! Pomodoro timer backed by taskwarrior start/stop (timewarrior logs the interval via its hook).
use std::{
  process::Command,
  time::{Duration, Instant},
};

use uuid::Uuid;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
  Idle,
  /// task picked, waiting for a preset
  Choose,
  Work,
  Break,
}

pub struct Pomodoro {
  pub phase: Phase,
  pub task: Option<(Uuid, String)>,
  pub started: Instant,
  pub length: Duration,
  pub completed: u32,
  /// (work, break) minutes
  pub presets: Vec<(u64, u64)>,
  pub selected: usize,
  rest: Duration,
  pub sound: String,
}

impl Pomodoro {
  pub fn new(presets: Vec<(u64, u64)>, sound: String) -> Self {
    Self {
      phase: Phase::Idle,
      task: None,
      started: Instant::now(),
      length: Duration::ZERO,
      completed: 0,
      presets,
      selected: 0,
      rest: Duration::ZERO,
      sound,
    }
  }

  pub fn remaining(&self) -> Duration {
    match self.phase {
      Phase::Idle | Phase::Choose => Duration::from_secs(self.presets[self.selected].0 * 60),
      _ => self.length.saturating_sub(self.started.elapsed()),
    }
  }

  pub fn progress(&self) -> f64 {
    if matches!(self.phase, Phase::Idle | Phase::Choose) || self.length.is_zero() {
      return 0.0;
    }
    1.0 - self.remaining().as_secs_f64() / self.length.as_secs_f64()
  }

  /// Pick `task` and wait for a preset. Stops any running session first.
  pub fn choose(&mut self, task_exe: &str, uuid: Uuid, description: String) {
    self.stop(task_exe);
    self.task = Some((uuid, description));
    self.phase = Phase::Choose;
  }

  pub fn select_next(&mut self) {
    self.selected = (self.selected + 1) % self.presets.len();
  }

  pub fn select_prev(&mut self) {
    self.selected = (self.selected + self.presets.len() - 1) % self.presets.len();
  }

  /// Start the work session with the selected preset (runs `task <uuid> start`).
  pub fn confirm(&mut self, task_exe: &str) {
    let Some((uuid, _)) = &self.task else { return };
    run(task_exe, &[&uuid.to_string(), "start"]);
    let (work, rest) = self.presets[self.selected];
    self.rest = Duration::from_secs(rest * 60);
    self.begin(Phase::Work, Duration::from_secs(work * 60));
  }

  /// Break time is tracked in timewarrior under the `break` tag.
  pub fn start_break(&mut self, task_exe: &str) {
    self.stop(task_exe);
    run("timew", &["start", "break"]);
    self.begin(Phase::Break, self.rest);
  }

  /// Stop whatever is running. A work session runs `task <uuid> stop` so timewarrior records it.
  pub fn stop(&mut self, task_exe: &str) {
    if self.phase == Phase::Work
      && let Some((uuid, _)) = &self.task
    {
      run(task_exe, &[&uuid.to_string(), "stop"]);
    }
    if self.phase == Phase::Break {
      run("timew", &["stop"]);
    }
    self.phase = Phase::Idle;
  }

  /// Call on every tick. Work -> Break -> Idle, with a sound + notification at each boundary.
  pub fn tick(&mut self, task_exe: &str) {
    if matches!(self.phase, Phase::Idle | Phase::Choose) || !self.remaining().is_zero() {
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
        self.stop(task_exe);
      }
      Phase::Idle | Phase::Choose => {}
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

pub fn run(task_exe: &str, args: &[&str]) {
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
    let mut p = Pomodoro::new(vec![(25, 5), (50, 10), (10, 2)], String::new());
    assert_eq!(p.remaining(), Duration::from_secs(1500));
    assert_eq!(p.progress(), 0.0);
    p.select_prev();
    assert_eq!(p.selected, 2);
    assert_eq!(p.remaining(), Duration::from_secs(600));
  }
}
