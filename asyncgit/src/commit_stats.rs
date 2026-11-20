use crate::{
	asyncjob::{AsyncJob, RunParams},
	sync::{get_commit_stats, CommitId, CommitStats, RepoPath},
	AsyncGitNotification, Result,
};

/// A coalescible batch of independent commit statistics requests.
#[derive(Clone)]
pub struct AsyncCommitStatsJob {
	/// Repository identity, used to reject stale completions.
	pub repo: RepoPath,
	/// Requested commits, in viewport order (one commit for Info).
	pub commits: Vec<CommitId>,
	/// One result per commit. `None` means statistics were unavailable.
	pub results: Vec<(CommitId, Option<CommitStats>)>,
}

impl AsyncCommitStatsJob {
	/// Create a batch for use with `AsyncSingleJob`.
	pub const fn new(repo: RepoPath, commits: Vec<CommitId>) -> Self {
		Self {
			repo,
			commits,
			results: Vec::new(),
		}
	}
}

impl AsyncJob for AsyncCommitStatsJob {
	type Notification = AsyncGitNotification;
	type Progress = ();

	fn run(
		&mut self,
		_params: RunParams<Self::Notification, Self::Progress>,
	) -> Result<Self::Notification> {
		self.results = self
			.commits
			.iter()
			.map(|&id| {
				let result = get_commit_stats(&self.repo, id)
					.map_err(|err| {
						log::error!("commit stats for {id}: {err}");
					})
					.ok();
				(id, result)
			})
			.collect();
		Ok(AsyncGitNotification::CommitStats)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		asyncjob::AsyncSingleJob,
		sync::tests::{repo_init, write_commit_file},
	};
	use std::time::Duration;

	#[test]
	fn batch_keeps_successes_and_failures_and_always_notifies() {
		let (dir, repo) = repo_init().unwrap();
		let first =
			write_commit_file(&repo, "file", "one\n", "first");
		let second = write_commit_file(
			&repo,
			"file",
			"two\nthree\n",
			"second",
		);
		let path: RepoPath = dir.path().to_path_buf().into();
		let (sender, receiver) = crossbeam_channel::unbounded();
		let job = AsyncSingleJob::new(sender);
		job.spawn(AsyncCommitStatsJob::new(
			path.clone(),
			vec![first, CommitId::default(), second],
		));
		assert_eq!(
			receiver.recv_timeout(Duration::from_secs(10)).unwrap(),
			AsyncGitNotification::CommitStats
		);
		let result = job.take_last().unwrap();
		assert_eq!(result.repo, path);
		assert_eq!(
			result.results,
			vec![
				(
					first,
					Some(CommitStats {
						insertions: 1,
						deletions: 0,
						is_merge: false
					})
				),
				(CommitId::default(), None),
				(
					second,
					Some(CommitStats {
						insertions: 2,
						deletions: 1,
						is_merge: false
					})
				),
			]
		);
		job.spawn(AsyncCommitStatsJob::new(
			dir.path().join("missing").into(),
			vec![first, second],
		));
		receiver.recv_timeout(Duration::from_secs(10)).unwrap();
		assert_eq!(
			job.take_last().unwrap().results,
			vec![(first, None), (second, None)]
		);
	}
}
