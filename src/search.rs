//! Fuzzy task search popup: type a query, an fzf-style subsequence match ranks `self.tasks` live.
use task_hookrs::task::Task;

pub struct TaskSearch {
  pub query: String,
  pub selected: usize,
}

impl TaskSearch {
  pub fn new() -> Self {
    Self { query: String::new(), selected: 0 }
  }
}

/// Description, project, tags, and every annotation description, joined with spaces.
pub fn haystack(task: &Task) -> String {
  let mut parts = vec![task.description().clone()];
  if let Some(project) = task.project() {
    parts.push(project.clone());
  }
  if let Some(tags) = task.tags() {
    parts.extend(tags.iter().cloned());
  }
  if let Some(annotations) = task.annotations() {
    parts.extend(annotations.iter().map(|a| a.description().clone()));
  }
  parts.join(" ")
}

/// Case-insensitive fzf-style match: every whitespace-separated query token must match as a
/// subsequence of `haystack`, else `None`. Higher score is a better match.
pub fn score(query: &str, haystack: &str) -> Option<i64> {
  let query = query.trim();
  if query.is_empty() {
    return Some(0);
  }
  let hay: Vec<char> = haystack.to_lowercase().chars().collect();
  query.split_whitespace().try_fold(0i64, |total, token| Some(total + score_token(&token.to_lowercase(), &hay)?))
}

/// Score one query token as a subsequence of `hay`, matching greedily left to right.
fn score_token(token: &str, hay: &[char]) -> Option<i64> {
  let mut score = 0i64;
  let mut search_from = 0usize;
  let mut prev_match: Option<usize> = None;
  for tc in token.chars() {
    let pos = search_from + hay[search_from..].iter().position(|&c| c == tc)?;
    let at_word_start = pos == 0 || !hay[pos - 1].is_alphanumeric();
    let consecutive = prev_match.is_some_and(|p| pos == p + 1);
    score += 1;
    if at_word_start {
      score += 8;
    }
    if consecutive {
      score += 5;
    } else if let Some(p) = prev_match {
      score -= (pos - p).min(3) as i64;
    }
    prev_match = Some(pos);
    search_from = pos + 1;
  }
  Some(score)
}

/// Indices into `tasks`, best match first. Ties keep report (input) order.
pub fn rank(query: &str, tasks: &[Task]) -> Vec<usize> {
  let mut scored: Vec<(usize, i64)> =
    tasks.iter().enumerate().filter_map(|(i, t)| score(query, &haystack(t)).map(|s| (i, s))).collect();
  scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
  scored.into_iter().map(|(i, _)| i).collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn fzf_matching_and_ranking() {
    // token only present in an annotation
    assert!(score("crashed", "fix login bug work +urgent server crashed after deploy").is_some());

    // tag and project match
    assert!(score("urgent", "fix login bug work +urgent").is_some());
    assert!(score("work", "fix login bug work +urgent").is_some());

    // multi-token subsequence match vs. an unrelated title
    assert!(score("fx lgn", "fix login bug").is_some());
    assert!(score("fx lgn", "buy groceries for the week").is_none());

    // word-start + contiguous match ranks above the same token scattered across other words
    let contiguous = score("log", "login system").unwrap();
    let scattered = score("log", "a lovely dog").unwrap();
    assert!(contiguous > scattered, "contiguous {contiguous} should outrank scattered {scattered}");

    // multi-token query requires every token to match
    assert!(score("fix nonexistentword", "fix login bug").is_none());

    // empty query matches everything with score 0
    assert_eq!(score("", "anything at all"), Some(0));
  }
}
