use anyhow::Result;
use ron::de::from_reader;
use serde::{Deserialize, Serialize};
use std::{
	fs::File,
	path::PathBuf,
	process::{Command, Stdio},
	sync::Arc,
};

use crate::args::get_app_config_path;

pub type SharedCommitHelpers = Arc<CommitHelpers>;

const COMMIT_HELPERS_FILENAME: &str = "commit_helpers.ron";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitHelper {
	/// Display name for the helper
	pub name: String,
	/// Command to execute (will be run through shell)
	pub command: String,
	/// Optional description of what this helper does
	pub description: Option<String>,
	/// Optional hotkey for quick access
	pub hotkey: Option<char>,
	/// Optional timeout in seconds (defaults to 30)
	pub timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CommitHelpers {
	pub helpers: Vec<CommitHelper>,
}

impl CommitHelpers {
	fn get_config_file() -> Result<PathBuf> {
		let app_home = get_app_config_path()?;
		let config_file = app_home.join(COMMIT_HELPERS_FILENAME);
		Ok(config_file)
	}

	pub fn init() -> Result<Self> {
		let config_file = Self::get_config_file()?;

		if config_file.exists() {
			let file = File::open(&config_file).map_err(|e| {
                anyhow::anyhow!("Failed to open commit_helpers.ron: {e}. Check file permissions.")
            })?;

			match from_reader::<_, Self>(file) {
				Ok(config) => {
					log::info!(
						"Loaded {} commit helpers from config",
						config.helpers.len()
					);
					Ok(config)
				}
				Err(e) => {
					log::error!(
						"Failed to parse commit_helpers.ron: {e}"
					);
					anyhow::bail!(
                        "Invalid RON syntax in commit_helpers.ron: {e}. \
                        Check the example file or remove the config to reset."
                    )
				}
			}
		} else {
			log::info!("No commit_helpers.ron found, using empty config. \
                       See commit_helpers.ron.example for configuration options.");
			Ok(Self::default())
		}
	}

	pub fn get_helpers(&self) -> &[CommitHelper] {
		&self.helpers
	}

	pub fn find_by_hotkey(&self, hotkey: char) -> Option<usize> {
		self.helpers.iter().position(|h| h.hotkey == Some(hotkey))
	}

	pub fn execute_helper(
		&self,
		helper_index: usize,
	) -> Result<String> {
		if helper_index >= self.helpers.len() {
			anyhow::bail!("Invalid helper index");
		}

		let helper = &self.helpers[helper_index];

		// Process template variables in command
		let processed_command =
			Self::process_template_variables(&helper.command)?;

		// Execute command through shell to support pipes and redirects
		let output = if cfg!(target_os = "windows") {
			Command::new("cmd")
				.args(["/C", &processed_command])
				.stdin(Stdio::null())
				.output()?
		} else {
			Command::new("sh")
				.args(["-c", &processed_command])
				.stdin(Stdio::null())
				.output()?
		};

		if !output.status.success() {
			let error = String::from_utf8_lossy(&output.stderr);
			anyhow::bail!("Command failed: {error}");
		}

		let result = String::from_utf8_lossy(&output.stdout)
			.trim()
			.to_string();

		if result.is_empty() {
			anyhow::bail!("Command returned empty output");
		}

		Ok(result)
	}

	fn process_template_variables(command: &str) -> Result<String> {
		let mut processed = command.to_string();

		// {staged_diff} - staged git diff
		if processed.contains("{staged_diff}") {
			let diff_output = Command::new("git")
				.args(["diff", "--staged", "--no-color"])
				.output()?;
			let diff = String::from_utf8_lossy(&diff_output.stdout);
			processed = processed.replace("{staged_diff}", &diff);
		}

		// {staged_files} - list of staged files
		if processed.contains("{staged_files}") {
			let files_output = Command::new("git")
				.args(["diff", "--staged", "--name-only"])
				.output()?;
			let files = String::from_utf8_lossy(&files_output.stdout);
			processed =
				processed.replace("{staged_files}", files.trim());
		}

		// {branch_name} - current branch name
		if processed.contains("{branch_name}") {
			let branch_output = Command::new("git")
				.args(["rev-parse", "--abbrev-ref", "HEAD"])
				.output()?;
			let branch =
				String::from_utf8_lossy(&branch_output.stdout);
			processed =
				processed.replace("{branch_name}", branch.trim());
		}

		Ok(processed)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::fs;
	use tempfile::TempDir;

	#[test]
	fn test_default_config() {
		let config = CommitHelpers::default();
		assert!(config.helpers.is_empty());
	}

	#[test]
	fn test_find_by_hotkey() {
		let config = CommitHelpers {
			helpers: vec![
				CommitHelper {
					name: "Test Helper 1".to_string(),
					command: "echo test1".to_string(),
					description: None,
					hotkey: Some('a'),
					timeout_secs: None,
				},
				CommitHelper {
					name: "Test Helper 2".to_string(),
					command: "echo test2".to_string(),
					description: None,
					hotkey: Some('b'),
					timeout_secs: None,
				},
			],
		};

		assert_eq!(config.find_by_hotkey('a'), Some(0));
		assert_eq!(config.find_by_hotkey('b'), Some(1));
		assert_eq!(config.find_by_hotkey('c'), None);
	}

	#[test]
	fn test_process_template_variables() {
		// Test basic template processing (these will use actual git commands)
		let result = CommitHelpers::process_template_variables(
			"test {branch_name} test",
		);
		assert!(result.is_ok());

		// Test no template variables
		let result = CommitHelpers::process_template_variables(
			"no templates here",
		)
		.unwrap();
		assert_eq!(result, "no templates here");
	}

	#[test]
	fn test_execute_helper_invalid_index() {
		let config = CommitHelpers::default();
		let result = config.execute_helper(0);
		assert!(result.is_err());
		assert!(result
			.unwrap_err()
			.to_string()
			.contains("Invalid helper index"));
	}

	#[test]
	fn test_execute_helper_success() {
		let config = CommitHelpers {
			helpers: vec![CommitHelper {
				name: "Echo Test".to_string(),
				command: "echo 'test message'".to_string(),
				description: None,
				hotkey: None,
				timeout_secs: None,
			}],
		};

		let result = config.execute_helper(0);
		assert!(result.is_ok());
		assert_eq!(result.unwrap().trim(), "test message");
	}

	#[test]
	fn test_execute_helper_empty_output() {
		let config = CommitHelpers {
			helpers: vec![CommitHelper {
				name: "Empty Test".to_string(),
				command: "true".to_string(), // Command that succeeds but produces no output
				description: None,
				hotkey: None,
				timeout_secs: None,
			}],
		};

		let result = config.execute_helper(0);
		assert!(result.is_err());
		assert!(result
			.unwrap_err()
			.to_string()
			.contains("Command returned empty output"));
	}

	#[test]
	fn test_config_file_parsing() {
		let temp_dir = TempDir::new().unwrap();
		let config_content = r#"CommitHelpers(
    helpers: [
        CommitHelper(
            name: "Test Helper",
            command: "echo test",
            description: Some("A test helper"),
            hotkey: Some('t'),
            timeout_secs: Some(15),
        )
    ]
)"#;

		let config_path = temp_dir.path().join("test_helpers.ron");
		fs::write(&config_path, config_content).unwrap();

		let file = std::fs::File::open(&config_path).unwrap();
		let config: CommitHelpers =
			ron::de::from_reader(file).unwrap();

		assert_eq!(config.helpers.len(), 1);
		assert_eq!(config.helpers[0].name, "Test Helper");
		assert_eq!(config.helpers[0].command, "echo test");
		assert_eq!(
			config.helpers[0].description,
			Some("A test helper".to_string())
		);
		assert_eq!(config.helpers[0].hotkey, Some('t'));
		assert_eq!(config.helpers[0].timeout_secs, Some(15));
	}
}
