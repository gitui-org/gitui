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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitHelpers {
    pub helpers: Vec<CommitHelper>,
}

impl Default for CommitHelpers {
    fn default() -> Self {
        Self {
            helpers: Vec::new(),
        }
    }
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
                anyhow::anyhow!("Failed to open commit_helpers.ron: {}. Check file permissions.", e)
            })?;
            
            match from_reader::<_, CommitHelpers>(file) {
                Ok(config) => {
                    log::info!("Loaded {} commit helpers from config", config.helpers.len());
                    Ok(config)
                },
                Err(e) => {
                    log::error!("Failed to parse commit_helpers.ron: {}", e);
                    anyhow::bail!(
                        "Invalid RON syntax in commit_helpers.ron: {}. \
                        Check the example file or remove the config to reset.", e
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

    pub fn execute_helper(&self, helper_index: usize) -> Result<String> {
        if helper_index >= self.helpers.len() {
            anyhow::bail!("Invalid helper index");
        }
        
        let helper = &self.helpers[helper_index];
        
        // Process template variables in command
        let processed_command = self.process_template_variables(&helper.command)?;
        
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
            anyhow::bail!("Command failed: {}", error);
        }
        
        let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
        
        if result.is_empty() {
            anyhow::bail!("Command returned empty output");
        }
        
        Ok(result)
    }

    fn process_template_variables(&self, command: &str) -> Result<String> {
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
            processed = processed.replace("{staged_files}", files.trim());
        }
        
        // {branch_name} - current branch name
        if processed.contains("{branch_name}") {
            let branch_output = Command::new("git")
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .output()?;
            let branch = String::from_utf8_lossy(&branch_output.stdout);
            processed = processed.replace("{branch_name}", branch.trim());
        }
        
        Ok(processed)
    }
}