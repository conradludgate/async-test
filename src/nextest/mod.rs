use std::time::{Duration, SystemTime};

use crate::TestInfo;

use self::reporter::{FinalStatusLevel, StatusLevel};

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
    /// The test was run once and was successful.
    Setup { duration: Duration },

    /// The test was run once, or possibly multiple times. All runs failed.
    Failure { status: &'a ExecuteStatus },
}

impl<'a> ExecutionDescription<'a> {
    /// Returns the final status level for this `ExecutionDescription`.
    pub fn final_status_level(&self) -> FinalStatusLevel {
        match self {
            ExecutionDescription::Success { status, .. } => {
                // Slow is higher priority than leaky, so return slow first here.
                if status.is_slow {
                    FinalStatusLevel::Slow
                } else {
                    FinalStatusLevel::Pass
                }
            }
            ExecutionDescription::Setup { .. } => FinalStatusLevel::Pass,
            // A flaky test implies that we print out retry information for it.
            ExecutionDescription::Failure { .. } => FinalStatusLevel::Fail,
        }
    }
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
    pub tests: Vec<TestInfo>,
    pub skip_count: usize,
}

impl TestList {
    /// Returns the total number of skipped tests.
    pub fn skip_count(&self) -> usize {
        self.skip_count
    }

    /// Returns the total number of tests that aren't skipped.
    ///
    /// It is always the case that `run_count + skip_count == test_count`.
    pub fn run_count(&self) -> usize {
        self.tests.len()
    }
}

impl<'a> ExecutionDescription<'a> {
    /// Returns the status level for this `ExecutionDescription`.
    pub fn status_level(&self) -> StatusLevel {
        match self {
            ExecutionDescription::Success { .. } | ExecutionDescription::Setup { .. } => {
                StatusLevel::Pass
            }
            ExecutionDescription::Failure { .. } => StatusLevel::Fail,
        }
    }
}

/// The reason for why a test doesn't match a filter.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum MismatchReason {
    /// This test does not match the run-ignored option in the filter.
    Ignored,

    /// This test does not match the provided string filters.
    String,

    /// This test does not match the provided expression filters.
    Expression,

    /// This test is in a different partition.
    Partition,
}
