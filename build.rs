use chrono::TimeZone;

fn get_git_hash() -> String {
	use std::process::Command;

	// Allow builds from `git archive` generated tarballs if output of `git get-tar-commit-id` is
	// set in an env var.
	if let Ok(commit) = std::env::var("BUILD_GIT_COMMIT_ID") {
		return commit[..7].to_string();
	};
	let commit = Command::new("git")
		.arg("rev-parse")
		.arg("--short=7")
		.arg("--verify")
		.arg("HEAD")
		.output();
	if let Ok(commit_output) = commit {
		let commit_string =
			String::from_utf8_lossy(&commit_output.stdout);

		return commit_string.lines().next().unwrap_or("").into();
	}

	panic!("Can not get git commit: {}", commit.unwrap_err());
}

fn main() {
	let now = match std::env::var("SOURCE_DATE_EPOCH") {
		Ok(val) => chrono::Local
			.timestamp_opt(val.parse::<i64>().unwrap(), 0)
			.unwrap(),
		Err(_) => chrono::Local::now(),
	};
	let build_date = now.date_naive();

	let build_name = if std::env::var("GITUI_RELEASE").is_ok() {
		format!(
			"{} {} ({})",
			env!("CARGO_PKG_VERSION"),
			build_date,
			get_git_hash()
		)
	} else {
		format!("nightly {} ({})", build_date, get_git_hash())
	};

	println!("cargo:warning=buildname '{build_name}'");
	println!("cargo:rustc-env=GITUI_BUILD_NAME={build_name}");
}
