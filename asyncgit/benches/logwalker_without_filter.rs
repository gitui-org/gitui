use asyncgit::sync::{CommitId, LogWalkerWithoutFilter};
use criterion::{
	criterion_group, criterion_main, BatchSize, Criterion,
};
use std::{
	env,
	hint::black_box,
	path::{Path, PathBuf},
};

const REPO_ENV_VAR: &str = "GITUI_BENCH_REPO";
const LIMIT_COUNT: usize = 3000;

fn bench_repo_path() -> PathBuf {
	env::var_os(REPO_ENV_VAR).map_or_else(
		|| {
			panic!(
				"{REPO_ENV_VAR} must point at the repository to benchmark, \
				for example: {REPO_ENV_VAR}=/path/to/repo cargo bench \
				-p asyncgit --bench logwalker_without_filter"
			);
		},
		PathBuf::from,
	)
}

fn open_repo(path: &Path) -> gix::Repository {
	gix::ThreadSafeRepository::discover_with_environment_overrides(
		path,
	)
	.unwrap_or_else(|err| {
		panic!(
			"failed to open repository from {REPO_ENV_VAR}={}: {err}",
			path.display()
		);
	})
	.into()
}

// Mirrors `AsyncLog::fetch_helper_without_filter`'s read loop, but without `thread::sleep`, so the
// timing focuses on reading the full history.
fn read_full_history(
	repo: &mut gix::Repository,
) -> asyncgit::Result<usize> {
	let mut entries = vec![CommitId::default(); LIMIT_COUNT];
	entries.clear();

	let mut commits: Vec<CommitId> = Vec::with_capacity(LIMIT_COUNT);
	let mut walker = LogWalkerWithoutFilter::new(repo, LIMIT_COUNT)?;

	loop {
		entries.clear();
		let read = walker.read(&mut entries)?;
		commits.extend(entries.iter());

		if read == 0 {
			break;
		}
	}

	let commit_count = commits.len();
	black_box(commits);

	Ok(commit_count)
}

fn logwalker_without_filter_full_history(c: &mut Criterion) {
	let repo_path = bench_repo_path();

	c.bench_function("logwalker_without_filter_full_history", |b| {
		b.iter_batched_ref(
			|| open_repo(&repo_path),
			|repo| {
				black_box(read_full_history(repo).unwrap_or_else(
					|err| {
						panic!("failed to read repository history: {err}");
					},
				));
			},
			BatchSize::PerIteration,
		);
	});
}

criterion_group!(benches, logwalker_without_filter_full_history);
criterion_main!(benches);
