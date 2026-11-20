use super::ItemBatch;
use crate::ui::style::Theme;
use asyncgit::{
	asyncjob::AsyncSingleJob,
	sync::{CommitId, CommitStats, RepoPath},
	AsyncCommitStatsJob, AsyncGitNotification, DiffLineType,
};
use crossbeam_channel::Sender;
use ratatui::{style::Style, text::Span};
use std::collections::{HashMap, HashSet};

/// Local to one list; only the Log tab enables it.
pub(super) struct ListStats {
	pub enabled: bool,
	pub cache: HashMap<CommitId, Option<CommitStats>>,
	pub job: AsyncSingleJob<AsyncCommitStatsJob>,
	repo: RepoPath,
	requested: Vec<CommitId>,
}

impl ListStats {
	pub fn new(
		repo: RepoPath,
		sender: Sender<AsyncGitNotification>,
	) -> Self {
		Self {
			enabled: false,
			cache: HashMap::new(),
			job: AsyncSingleJob::new(sender),
			repo,
			requested: Vec::new(),
		}
	}

	pub fn set_enabled(&mut self, enabled: bool) {
		self.enabled = enabled;
		if !enabled {
			self.job.cancel();
			self.requested.clear();
		}
	}

	pub fn retain(&mut self, items: &ItemBatch) {
		if self.cache.is_empty() {
			return;
		}
		let ids: HashSet<_> =
			items.iter().map(|entry| entry.id).collect();
		self.cache.retain(|id, _| ids.contains(id));
	}

	pub fn update(
		&mut self,
		repo: &RepoPath,
		items: &ItemBatch,
		top: usize,
		height: usize,
	) {
		if self.repo != *repo {
			self.job.cancel();
			self.cache.clear();
			self.requested.clear();
			self.repo = repo.clone();
		}
		if let Some(job) = self.job.take_last() {
			self.apply(job);
		}
		self.retain(items);
		if !self.enabled {
			return;
		}
		let missing: Vec<_> = items
			.iter()
			.skip(top)
			.take(height)
			.map(|entry| entry.id)
			.filter(|id| !self.cache.contains_key(id))
			.collect();
		if missing.is_empty() {
			self.job.cancel();
			self.requested.clear();
		} else if missing != self.requested {
			self.requested.clone_from(&missing);
			self.job.spawn(AsyncCommitStatsJob::new(
				repo.clone(),
				missing,
			));
		}
	}

	fn apply(&mut self, job: AsyncCommitStatsJob) {
		if job.repo == self.repo {
			if job.commits == self.requested {
				self.requested.clear();
			}
			self.cache.extend(job.results);
		}
	}

	pub fn spans(
		&self,
		id: CommitId,
		(added_width, removed_width): (usize, usize),
		normal: bool,
		selected: bool,
		theme: &Theme,
	) -> Vec<Span<'static>> {
		let text = if normal {
			theme.text(true, selected)
		} else {
			theme.commit_unhighlighted()
		};
		let style = |kind| {
			if normal {
				theme.diff_line(kind, selected)
			} else {
				theme.commit_unhighlighted()
			}
		};
		match self.cache.get(&id) {
			Some(Some(stats)) => vec![
				Span::styled(
					format!(
						"{:>added_width$}",
						format!("+{}", stats.insertions)
					),
					style(DiffLineType::Add),
				),
				Span::styled(
					" ",
					if normal { text } else { Style::default() },
				),
				Span::styled(
					format!(
						"{:>removed_width$}",
						format!("-{}", stats.deletions)
					),
					style(DiffLineType::Delete),
				),
			],
			result => vec![Span::styled(
				format!(
					"{:<width$}",
					if result.is_some() { "—" } else { "…" },
					width = added_width + removed_width + 1
				),
				text,
			)],
		}
	}

	pub fn widths(
		&self,
		items: &ItemBatch,
		top: usize,
		height: usize,
	) -> Option<(usize, usize)> {
		self.enabled.then(|| {
			items
				.iter()
				.skip(top)
				.take(height)
				.filter_map(|entry| {
					self.cache.get(&entry.id).copied().flatten()
				})
				.fold((2, 2), |(added, removed), stats| {
					(
						added.max(
							stats.insertions.to_string().len() + 1,
						),
						removed.max(
							stats.deletions.to_string().len() + 1,
						),
					)
				})
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::components::commitlist::tests::build_commit_list_with_some_commits;
	use std::time::Duration;

	#[test]
	fn visible_rows_only_cache_reuse_and_batch_eviction() {
		let list = build_commit_list_with_some_commits();
		let items = &list.items;
		let (sender, receiver) = crossbeam_channel::unbounded();
		let dir = tempfile::tempdir().unwrap();
		let repo = dir.path().to_path_buf().into();
		let mut stats = ListStats::new(repo, sender);
		let repo = stats.repo.clone();
		stats.update(&repo, items, 0, 3);
		assert!(stats.requested.is_empty()); // Default off, as in other lists.
		stats.set_enabled(true);
		stats.update(&repo, items, 2, 3);
		let expected: Vec<_> =
			items.iter().skip(2).take(3).map(|e| e.id).collect();
		assert_eq!(stats.requested, expected);
		receiver.recv_timeout(Duration::from_secs(10)).unwrap();
		stats.update(&repo, items, 2, 3);
		assert_eq!(stats.cache.len(), 3);
		assert!(expected
			.iter()
			.all(|id| stats.cache.get(id) == Some(&None)));
		for _ in 0..5 {
			stats.update(&repo, items, 2, 3);
			assert!(stats.requested.is_empty());
		}
		assert!(receiver.is_empty());
		stats.set_enabled(false);
		stats.set_enabled(true);
		stats.update(&repo, items, 2, 3);
		assert!(stats.requested.is_empty());
		stats.update(&repo, items, 3, 3);
		assert_eq!(
			stats.requested,
			vec![items.iter().nth(5).unwrap().id]
		);
		receiver.recv_timeout(Duration::from_secs(10)).unwrap();
		stats.update(&repo, items, 3, 3);
		assert_eq!(stats.cache.len(), 4);
		stats.retain(&ItemBatch::default());
		assert!(stats.cache.is_empty());
	}

	#[test]
	fn stale_completions_match_repository_and_loaded_batch() {
		let list = build_commit_list_with_some_commits();
		let (sender, _receiver) = crossbeam_channel::unbounded();
		let repo: RepoPath = "/first".into();
		let mut stats = ListStats::new(repo.clone(), sender);
		let id = list.items.iter().next().unwrap().id;
		let other = list.items.iter().nth(1).unwrap().id;
		stats.requested = vec![other];
		let mut completion =
			AsyncCommitStatsJob::new("/second".into(), vec![id]);
		completion.results = vec![(id, Some(CommitStats::default()))];
		stats.apply(completion.clone());
		assert!(stats.cache.is_empty());
		completion.repo = repo.clone();
		stats.apply(completion);
		assert!(stats.cache.contains_key(&id));
		assert_eq!(stats.requested, [other]);
		stats.update(&repo, &ItemBatch::default(), 0, 3);
		assert!(stats.cache.is_empty());
		// Same commit IDs in another repository cannot reuse old results.
		stats.cache.insert(id, None);
		stats.update(&"/second".into(), &list.items, 0, 3);
		assert!(stats.cache.is_empty());
	}

	#[test]
	fn disabling_during_loading_drops_queued_work() {
		let list = build_commit_list_with_some_commits();
		let (sender, receiver) = crossbeam_channel::unbounded();
		let dir = tempfile::tempdir().unwrap();
		let repo = dir.path().to_path_buf().into();
		let mut stats = ListStats::new(repo, sender);
		let repo = stats.repo.clone();
		stats.set_enabled(true);
		stats.update(&repo, &list.items, 0, 2);
		stats.set_enabled(false);
		assert!(stats.requested.is_empty());
		assert!(!stats.job.cancel()); // No queued list work remains.
		receiver.recv_timeout(Duration::from_secs(10)).unwrap();
		stats.update(&repo, &list.items, 4, 3);
		assert_eq!(stats.cache.len(), 2); // In-flight result can finish.
		assert!(stats.requested.is_empty());
		stats.set_enabled(true);
		stats.update(&repo, &list.items, 4, 3);
		assert_eq!(stats.requested.len(), 3);
		receiver.recv_timeout(Duration::from_secs(10)).unwrap();
		stats.update(&repo, &list.items, 4, 3);
		assert_eq!(stats.cache.len(), 5);
	}
}
