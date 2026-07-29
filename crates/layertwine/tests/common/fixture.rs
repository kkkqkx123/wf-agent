//! Fixture definitions and generation for E2E tests
#![allow(dead_code, unused_imports, unused_variables)]

use layertwine::api::{ApiService, ServiceConfig};
use layertwine::core::file_node::FileNode;
use layertwine::core::partition::Partition;
use layertwine::core::snapshot::Snapshot;
use layertwine::core::types::{DiffOp, Hunk, LayerType, LineDiff, PartitionType};
use layertwine::storage::SqliteStorage;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Test environment configuration
#[derive(Debug, Clone)]
pub struct TestConfig {
    pub db_name: String,
    pub enable_git: bool,
    pub verbose: bool,
}

impl Default for TestConfig {
    fn default() -> Self {
        TestConfig {
            db_name: "layertwine-test.db".into(),
            enable_git: false,
            verbose: true,
        }
    }
}

/// Test environment setup
pub struct TestEnvironment {
    pub temp_dir: TempDir,
    pub db_path: PathBuf,
    pub git_repo: Option<PathBuf>,
    pub config: TestConfig,
    pub storage: SqliteStorage,
    pub api: ApiService,
}

impl TestEnvironment {
    /// Create a new test environment
    pub fn new(config: TestConfig) -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let db_path = temp_dir.path().join(&config.db_name);

        // Initialize storage
        let storage = SqliteStorage::new_full(&db_path).expect("Failed to initialize storage");

        // Create API service
        let api = ApiService::open(ServiceConfig {
            db_path: db_path.to_string_lossy().to_string(),
        })
        .expect("Failed to create API service");

        TestEnvironment {
            temp_dir,
            db_path,
            git_repo: None,
            config,
            storage,
            api,
        }
    }

    /// Create test environment with Git repository
    pub fn with_git(config: TestConfig) -> Self {
        let mut env = Self::new(config);
        env.setup_git_repo();
        env
    }

    /// Setup a Git repository in the test environment
    fn setup_git_repo(&mut self) {
        use std::process::Command;

        let git_path = self.temp_dir.path().join("git-repo");
        std::fs::create_dir_all(&git_path).expect("Failed to create git repo dir");

        // Initialize git repo
        Command::new("git")
            .args(["init"])
            .current_dir(&git_path)
            .output()
            .expect("Failed to initialize git repo");

        // Configure git
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&git_path)
            .output()
            .expect("Failed to configure git");

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&git_path)
            .output()
            .expect("Failed to configure git");

        self.git_repo = Some(git_path);
    }

    /// Get the database path as string
    pub fn db_path_str(&self) -> String {
        self.db_path.to_string_lossy().to_string()
    }

    /// Get the git repo path as string
    pub fn git_repo_path(&self) -> Option<String> {
        self.git_repo
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
    }

    /// Clean up the test environment
    pub fn cleanup(self) {
        // TempDir will be cleaned up automatically when dropped
    }
}

/// Test scenarios
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestScenario {
    // Basic scenarios
    EmptyFile,
    SingleLineFile,
    MultiLineFile,

    // Edit scenarios
    SimpleInsert,
    SimpleDelete,
    SimpleReplace,
    MultipleEdits,

    // Collaboration scenarios
    TwoAgentsParallel,
    ThreeAgentsSequential,

    // Conflict scenarios
    SameLineConflict,
    AdjacentLineConflict,
    OverlappingEdits,

    // Branch scenarios
    SimpleBranch,
    MergeConflict,
}

impl TestScenario {
    /// Get the initial content for this scenario
    pub fn initial_content(&self) -> String {
        match self {
            TestScenario::EmptyFile => String::new(),
            TestScenario::SingleLineFile => "Hello, World!\n".to_string(),
            TestScenario::MultiLineFile => "Line 1\nLine 2\nLine 3\nLine 4\nLine 5\n".to_string(),
            TestScenario::SimpleInsert => "Line 1\nLine 3\n".to_string(),
            TestScenario::SimpleDelete => "Line 1\nLine 2\nLine 3\n".to_string(),
            TestScenario::SimpleReplace => "Old Line\n".to_string(),
            TestScenario::MultipleEdits => "Line 1\nLine 2\nLine 3\nLine 4\nLine 5\n".to_string(),
            TestScenario::TwoAgentsParallel => "Base line\n".to_string(),
            TestScenario::ThreeAgentsSequential => "Base content\n".to_string(),
            TestScenario::SameLineConflict => "Line 1\nLine 2\nLine 3\n".to_string(),
            TestScenario::AdjacentLineConflict => "Line 1\nLine 2\nLine 3\n".to_string(),
            TestScenario::OverlappingEdits => "Line 1\nLine 2\nLine 3\nLine 4\n".to_string(),
            TestScenario::SimpleBranch => "Initial content\n".to_string(),
            TestScenario::MergeConflict => "Base line\n".to_string(),
        }
    }

    /// Get the file name for this scenario
    pub fn file_name(&self) -> String {
        match self {
            TestScenario::EmptyFile => "empty.txt".to_string(),
            TestScenario::SingleLineFile => "single.txt".to_string(),
            TestScenario::MultiLineFile => "multi.txt".to_string(),
            _ => "test.txt".to_string(),
        }
    }
}

/// Test fixture data
pub struct TestFixture {
    pub file_content: String,
    pub file_path: PathBuf,
    pub delta_ops: Vec<DiffOp>,
}

impl TestFixture {
    /// Create a new fixture with the given content
    pub fn new(content: String, file_path: PathBuf) -> Self {
        TestFixture {
            file_content: content,
            file_path,
            delta_ops: vec![],
        }
    }

    /// Create a fixture from a scenario
    pub fn from_scenario(scenario: TestScenario) -> Self {
        let content = scenario.initial_content();
        let file_path = PathBuf::from(scenario.file_name());
        TestFixture::new(content, file_path)
    }

    /// Add a delta operation to the fixture
    pub fn add_delta_op(&mut self, op: DiffOp) {
        self.delta_ops.push(op);
    }

    /// Get the content as bytes
    pub fn content_bytes(&self) -> Vec<u8> {
        self.file_content.as_bytes().to_vec()
    }
}

/// Create a simple insert operation
pub fn create_insert_op(line_num: usize, lines: Vec<String>) -> DiffOp {
    DiffOp::Insert {
        new_start: line_num as u32,
        lines,
    }
}

/// Create a simple delete operation
pub fn create_delete_op(line_num: usize, count: u32) -> DiffOp {
    DiffOp::Delete {
        old_start: line_num as u32,
        count,
    }
}

/// Create a simple replace operation
pub fn create_replace_op(line_num: usize, old_count: u32, lines: Vec<String>) -> DiffOp {
    DiffOp::Replace {
        old_start: line_num as u32,
        old_count,
        new_start: line_num as u32,
        lines,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_test_environment_creation() {
        let config = TestConfig::default();
        let env = TestEnvironment::new(config);

        assert!(env.db_path.exists());
        assert!(env.git_repo.is_none());
    }

    #[test]
    fn test_test_environment_with_git() {
        let config = TestConfig::default();
        let env = TestEnvironment::with_git(config);

        assert!(env.db_path.exists());
        assert!(env.git_repo.is_some());
        assert!(env.git_repo.unwrap().exists());
    }

    #[test]
    fn test_scenario_initial_content() {
        assert_eq!(TestScenario::EmptyFile.initial_content(), "");
        assert_eq!(
            TestScenario::SingleLineFile.initial_content(),
            "Hello, World!\n"
        );
        assert!(TestScenario::MultiLineFile
            .initial_content()
            .contains("Line 1"));
    }

    #[test]
    fn test_fixture_creation() {
        let fixture = TestFixture::from_scenario(TestScenario::SingleLineFile);
        assert_eq!(fixture.file_content, "Hello, World!\n");
        assert_eq!(fixture.file_path, PathBuf::from("single.txt"));
    }
}
