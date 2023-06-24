use std::{
    sync::OnceLock,
    time::{Duration, SystemTime},
};

use bytes::Bytes;

use crate::TestInfo;

use self::reporter::StatusLevel;

pub mod reporter;

/// Information about a single execution of a test.
#[derive(Clone, Debug)]
pub struct ExecuteStatus {
    // /// Standard output for this test.
    // pub stdout: Bytes,
    // /// Standard error for this test.
    // pub stderr: Bytes,
    pub output: Option<String>,
    /// The result of execution this test: pass, fail or execution error.
    pub result: ExecutionResult,
    /// The time at which the test started.
    pub start_time: SystemTime,
    /// The time it took for the test to run.
    pub time_taken: Duration,
    /// Whether this test counts as slow.
    pub is_slow: bool,
    /// The delay will be non-zero if this is a retry and delay was specified.
    pub delay_before_start: Duration,
}

/// Whether a test passed, failed or an error occurred while executing the test.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ExecutionResult {
    /// The test passed.
    Pass,
    /// The test failed.
    Fail,
    /// The test was terminated due to timeout.
    Timeout,
}

/// Statistics for a test run.
#[derive(Copy, Clone, Default, Debug, Eq, PartialEq)]
pub struct RunStats {
    /// The total number of tests that were expected to be run at the beginning.
    ///
    /// If the test run is canceled, this will be more than `finished_count` at the end.
    pub initial_run_count: usize,

    /// The total number of tests that finished running.
    pub finished_count: usize,

    /// The number of tests that passed. Includes `passed_slow`, `flaky` and `leaky`.
    pub passed: usize,

    /// The number of slow tests that passed.
    pub passed_slow: usize,

    // /// The number of tests that passed on retry.
    // pub flaky: usize,
    /// The number of tests that failed.
    pub failed: usize,

    /// The number of failed tests that were slow.
    pub failed_slow: usize,

    /// The number of tests that timed out.
    pub timed_out: usize,

    /// The number of tests that were skipped.
    pub skipped: usize,
}

impl RunStats {
    /// Returns true if any tests failed or were timed out.
    #[inline]
    pub fn any_failed(&self) -> bool {
        self.failed > 0 || self.timed_out > 0
    }
}

/// A description of test executions obtained from `ExecuteStatuses`.
///
/// This can be used to quickly determine whether a test passed or failed
#[derive(Copy, Clone, Debug)]
pub enum ExecutionDescription<'a> {
    /// The test was run once and was successful.
    Success {
        /// The status of the test.
        status: &'a ExecuteStatus,
    },

    /// The test was run once, or possibly multiple times. All runs failed.
    Failure { status: &'a ExecuteStatus },
}

/// Represents a single test with its associated binary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestInstance {
    /// The name of the test.
    pub name: String,
    // /// Information about the test suite.
    // pub suite_info: &'a RustTestSuite<'a>,

    // /// Information about the test.
    // pub test_info: &'a RustTestCaseSummary,
}

#[allow(clippy::len_without_is_empty)] // RunStatuses is never empty
impl ExecuteStatus {
    /// Returns a description of self.
    pub fn describe(&self) -> ExecutionDescription<'_> {
        if self.result == ExecutionResult::Pass {
            ExecutionDescription::Success { status: self }
        } else {
            ExecutionDescription::Failure { status: self }
        }
    }
}

/// List of test instances, obtained by querying the [`RustTestArtifact`] instances generated by Cargo.
#[derive(Clone, Debug)]
pub(crate) struct TestList {
    // test_count: usize,
    pub(crate) tests: Vec<TestInfo>,
    // rust_suites: RustTestSuite<'g>,
    // Computed on first access.
    pub skip_count: OnceLock<usize>,
}

impl TestList {
    /// Returns the total number of skipped tests.
    pub fn skip_count(&self) -> usize {
        *self.skip_count.get_or_init(|| {
            // self.iter_tests()
            //     .filter(|instance| !instance.test_info.filter_match.is_match())
            //     .count()
            0
        })
    }

    /// Returns the total number of tests that aren't skipped.
    ///
    /// It is always the case that `run_count + skip_count == test_count`.
    pub fn run_count(&self) -> usize {
        self.tests.len() - self.skip_count()
    }
}

// /// A suite of tests within a single Rust test binary.
// ///
// /// This is a representation of [`nextest_metadata::RustTestSuiteSummary`] used internally by the runner.
// #[derive(Clone, Debug, Eq, PartialEq)]
// pub struct RustTestSuite<'g> {

//     /// Package metadata.
//     pub package: PackageMetadata<'g>,

//     /// The unique binary name defined in `Cargo.toml` or inferred by the filename.
//     pub binary_name: String,

//     /// The kind of Rust test binary this is.
//     pub kind: RustTestBinaryKind,

//     /// The working directory that this test binary will be executed in. If None, the current directory
//     /// will not be changed.
//     pub cwd: Utf8PathBuf,

//     /// The platform the test suite is for (host or target).
//     pub build_platform: BuildPlatform,

//     /// Non-test binaries corresponding to this test suite (name, path).
//     pub non_test_binaries: BTreeSet<(String, Utf8PathBuf)>,

//     /// Test suite status and test case names.
//     pub status: RustTestSuiteStatus,
// }
// //

impl<'a> ExecutionDescription<'a> {
    /// Returns the status level for this `ExecutionDescription`.
    pub fn status_level(&self) -> StatusLevel {
        match self {
            ExecutionDescription::Success { .. } => StatusLevel::Pass,
            ExecutionDescription::Failure { .. } => StatusLevel::Fail,
        }
    }
}
