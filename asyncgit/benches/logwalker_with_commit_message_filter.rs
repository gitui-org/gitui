use asyncgit::sync::{
	filter_commit_by_search, CommitId, LogFilterSearch,
	LogFilterSearchOptions, LogWalker, SearchFields, SearchOptions,
	SharedCommitFilterFn,
};
use criterion::{
	criterion_group, criterion_main, BatchSize, Criterion,
};
use git2::{Repository, RepositoryOpenFlags};
use std::{
	env,
	hint::black_box,
	path::{Path, PathBuf},
};

const REPO_ENV_VAR: &str = "GITUI_BENCH_REPO";
const FILTER_ENV_VAR: &str = "GITUI_BENCH_COMMIT_MESSAGE_FILTER";
const DEFAULT_FILTER: &str = "fix";
const LIMIT_COUNT: usize = 3000;

fn bench_repo_path() -> PathBuf {
	env::var_os(REPO_ENV_VAR).map_or_else(
		|| {
			panic!(
				"{REPO_ENV_VAR} must point at the repository to benchmark, \
				for example: {REPO_ENV_VAR}=/path/to/repo cargo bench \
				-p asyncgit --bench logwalker_with_commit_message_filter"
			);
		},
		PathBuf::from,
	)
}

fn bench_filter_pattern() -> String {
	env::var(FILTER_ENV_VAR)
		.unwrap_or_else(|_| String::from(DEFAULT_FILTER))
}

fn open_repo(path: &Path) -> Repository {
	Repository::open_ext(
		path,
		RepositoryOpenFlags::FROM_ENV,
		Vec::<&Path>::new(),
	)
	.unwrap_or_else(|err| {
		panic!(
			"failed to open repository from {REPO_ENV_VAR}={}: {err}",
			path.display()
		);
	})
}

fn commit_message_filter(
	search_pattern: String,
) -> SharedCommitFilterFn {
	filter_commit_by_search(LogFilterSearch::new(
		LogFilterSearchOptions {
			search_pattern,
			fields: SearchFields::MESSAGE_SUMMARY
				| SearchFields::MESSAGE_BODY,
			options: SearchOptions::default(),
		},
	))
}

// Mirrors `AsyncLog::fetch_helper_with_filter`'s read loop, but without `thread::sleep`, so the
// timing focuses on reading and filtering the full history.
fn read_filtered_history(
	repo: &Repository,
	filter: SharedCommitFilterFn,
) -> asyncgit::Result<usize> {
	let mut entries = vec![CommitId::default(); LIMIT_COUNT];
	entries.clear();

	let mut commits = Vec::with_capacity(LIMIT_COUNT);
	let mut walker =
		LogWalker::new(repo, LIMIT_COUNT)?.filter(Some(filter));

	loop {
		entries.clear();
		let read = walker.read(&mut entries)?;
		commits.extend(entries.iter().copied());

		if read == 0 {
			break;
		}
	}

	let commit_count = commits.len();
	black_box(commits);

	Ok(commit_count)
}

fn logwalker_with_commit_message_filter_full_history(
	c: &mut Criterion,
) {
	let repo_path = bench_repo_path();
	let filter = commit_message_filter(bench_filter_pattern());

	c.bench_function(
		"logwalker_with_commit_message_filter_full_history",
		|b| {
			b.iter_batched(
				|| (open_repo(&repo_path), filter.clone()),
				|(repo, filter)| {
					black_box(
						read_filtered_history(&repo, filter)
							.unwrap_or_else(|err| {
								panic!(
								"failed to read and filter repository history: {err}"
							);
							}),
					);
				},
				BatchSize::PerIteration,
			);
		},
	);
}

criterion_group!(
	benches,
	logwalker_with_commit_message_filter_full_history
);
criterion_main!(benches);
