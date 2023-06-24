// Copyright (c) The nextest Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Metadata management.

#[cfg(any(unix, windows))]
// use crate::runner::AbortStatus;
// use crate::{
//     config::{NextestJunitConfig, NextestProfile},
//     errors::WriteEventError,
//     list::TestInstance,
//     reporter::TestEvent,
//     runner::{ExecuteStatus, ExecutionDescription, ExecutionResult},
// };
// use camino::Utf8PathBuf;
use chrono::{DateTime, FixedOffset, Utc};
use debug_ignore::DebugIgnore;
use quick_junit::{NonSuccessKind, Report, TestCase, TestCaseStatus, TestSuite};
use std::{borrow::Cow, collections::HashMap, fs::File, path::PathBuf, time::SystemTime};
use thiserror::Error;

use crate::nextest::{ExecuteStatus, ExecutionResult};

use super::TestEvent;

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct EventAggregator<'cfg> {
    // store_dir: PathBuf,
    // TODO: log information in a JSONable report (converting that to XML later) instead of directly
    // writing it to XML
    junit: Option<MetadataJunit<'cfg>>,
}

impl<'cfg> EventAggregator<'cfg> {
    pub(crate) fn new_junit(profile: NextestJunitConfig<'cfg>) -> Self {
        Self {
            // store_dir: profile.store_dir().to_owned(),
            junit: Some(MetadataJunit::new(profile)),
        }
    }
    pub(crate) fn new() -> Self {
        Self {
            // store_dir: profile.store_dir().to_owned(),
            junit: None,
        }
    }

    pub(crate) fn write_event(&mut self, event: TestEvent<'cfg>) -> Result<(), WriteEventError> {
        if let Some(junit) = &mut self.junit {
            junit.write_event(event)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct MetadataJunit<'cfg> {
    config: NextestJunitConfig<'cfg>,
    test_suites: DebugIgnore<HashMap<&'cfg str, TestSuite>>,
}

/// An error that occurs while writing an event.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum WriteEventError {
    /// An error occurred while writing the event to the provided output.
    #[error("error writing to output")]
    Io(#[source] std::io::Error),

    /// An error occurred while operating on the file system.
    #[error("error operating on path {file}")]
    Fs {
        /// The file being operated on.
        file: PathBuf,

        /// The underlying IO error.
        #[source]
        error: std::io::Error,
    },

    /// An error occurred while producing JUnit XML.
    #[error("error writing JUnit output to {file}")]
    Junit {
        /// The output file.
        file: PathBuf,

        /// The underlying error.
        #[source]
        error: quick_junit::SerializeError,
    },
}

#[derive(Clone, Debug)]
pub struct NextestJunitConfig<'cfg> {
    path: PathBuf,
    report_name: &'cfg str,
    store_success_output: bool,
    store_failure_output: bool,
}

impl<'cfg> MetadataJunit<'cfg> {
    fn new(config: NextestJunitConfig<'cfg>) -> Self {
        Self {
            config,
            test_suites: DebugIgnore(HashMap::new()),
        }
    }

    pub(crate) fn write_event(&mut self, event: TestEvent<'cfg>) -> Result<(), WriteEventError> {
        match event {
            TestEvent::RunStarted { .. }
            | TestEvent::RunPaused { .. }
            | TestEvent::RunContinued { .. } => {}
            TestEvent::TestStarted { .. } => {}
            TestEvent::TestSlow { .. } => {}
            TestEvent::TestFinished {
                test_instance,
                run_status,
                junit_store_success_output,
                junit_store_failure_output,
                ..
            } => {
                fn kind_ty(run_status: &ExecuteStatus) -> (NonSuccessKind, Cow<'static, str>) {
                    match run_status.result {
                        ExecutionResult::Fail => (NonSuccessKind::Failure, "test failure".into()),
                        ExecutionResult::Timeout => {
                            (NonSuccessKind::Failure, "test timeout".into())
                        }
                        ExecutionResult::Pass => {
                            unreachable!("this is a failure status")
                        }
                    }
                }

                // let testsuite = self.testsuite_for(test_instance);
                let testsuite = self
                    .test_suites
                    .entry("")
                    .or_insert_with(|| TestSuite::new("test"));

                let status = run_status.result;

                let testcase_status = match run_status.result {
                    ExecutionResult::Pass => TestCaseStatus::success(),
                    ExecutionResult::Fail => {
                        let mut testcase_status =
                            TestCaseStatus::non_success(NonSuccessKind::Failure);
                        testcase_status.set_type("test failure");
                        testcase_status
                    }
                    ExecutionResult::Timeout => {
                        let mut testcase_status =
                            TestCaseStatus::non_success(NonSuccessKind::Failure);
                        testcase_status.set_type("test timeout");
                        testcase_status
                    }
                };

                let mut testcase = TestCase::new(test_instance.name, testcase_status);
                testcase
                    .set_classname("test")
                    .set_timestamp(to_datetime(run_status.start_time))
                    .set_time(run_status.time_taken);

                // TODO: allure seems to want the output to be in a format where text files are
                // written out to disk:
                // https://github.com/allure-framework/allure2/blob/master/plugins/junit-xml-plugin/src/main/java/io/qameta/allure/junitxml/JunitXmlPlugin.java#L192-L196
                // we may have to update this format to handle that.
                let is_success = status == ExecutionResult::Pass;
                if !is_success {
                    if let Some(description) = &run_status.output {
                        testcase.status.set_description(description);
                    }
                }

                if (junit_store_success_output && is_success)
                    || (junit_store_failure_output && !is_success)
                {
                    if let Some(output) = &run_status.output {
                        testcase.set_system_out_lossy(output);
                    }
                }

                testsuite.add_test_case(testcase);
            }
            TestEvent::TestSkipped { .. } => {
                // TODO: report skipped tests? causes issues if we want to aggregate runs across
                // skipped and non-skipped tests. Probably needs to be made configurable.

                // let testsuite = self.testsuite_for(test_instance);
                //
                // let mut testcase_status = TestcaseStatus::skipped();
                // testcase_status.set_message(format!("Skipped: {}", reason));
                // let testcase = Testcase::new(test_instance.name, testcase_status);
                //
                // testsuite.add_testcase(testcase);
            }
            TestEvent::RunBeginCancel { .. } => {}
            TestEvent::RunFinished {
                run_id,
                start_time,
                elapsed,
                ..
            } => {
                // Write out the report to the given file.
                let mut report = Report::new("report");
                report
                    .set_uuid(run_id)
                    .set_timestamp(to_datetime(start_time))
                    .set_time(elapsed)
                    .add_test_suites(self.test_suites.drain().map(|(_, testsuite)| testsuite));

                let junit_path = &self.config.path;
                let junit_dir = junit_path.parent().expect("junit path must have a parent");
                std::fs::create_dir_all(junit_dir).map_err(|error| WriteEventError::Fs {
                    file: junit_dir.to_path_buf(),
                    error,
                })?;

                let f = File::create(junit_path).map_err(|error| WriteEventError::Fs {
                    file: junit_path.to_path_buf(),
                    error,
                })?;
                report
                    .serialize(f)
                    .map_err(|error| WriteEventError::Junit {
                        file: junit_path.to_path_buf(),
                        error,
                    })?;
            }
        }

        Ok(())
    }

    // fn testsuite_for(&mut self, test_instance: TestInstance<'cfg>) -> &mut TestSuite {
    //     self.test_suites
    //         .entry(test_instance.suite_info.binary_id.as_str())
    //         .or_insert_with(|| TestSuite::new(test_instance.suite_info.binary_id.as_str()))
    // }
}

fn to_datetime(system_time: SystemTime) -> DateTime<FixedOffset> {
    // Serialize using UTC.
    let datetime = DateTime::<Utc>::from(system_time);
    datetime.into()
}

// // This regex works for the default panic handler for Rust -- other panic handlers may not work,
// // which is why this is heuristic.
// static PANICKED_AT_REGEX_STR: &str = "^thread '([^']+)' panicked at '";
// static PANICKED_AT_REGEX: Lazy<Regex> = Lazy::new(|| {
//     let mut builder = RegexBuilder::new(PANICKED_AT_REGEX_STR);
//     builder.multi_line(true);
//     builder.build().unwrap()
// });

// static ERROR_REGEX_STR: &str = "^Error: ";
// static ERROR_REGEX: Lazy<Regex> = Lazy::new(|| {
//     let mut builder = RegexBuilder::new(ERROR_REGEX_STR);
//     builder.multi_line(true);
//     builder.build().unwrap()
// });

// #[allow(unused_variables)]
// /// Not part of the public API: only used for testing.
// #[doc(hidden)]
// pub fn heuristic_extract_description<'a>(
//     exec_result: ExecutionResult,
//     stdout: &'a str,
//     stderr: &'a str,
// ) -> Option<String> {
//     // Try the heuristic stack trace extraction first as they're the more common kinds of test.
//     if let Some(description) = heuristic_stack_trace(stderr) {
//         return Some(description);
//     }
//     if let Some(description) = heuristic_error_str(stderr) {
//         return Some(description);
//     }
//     heuristic_should_panic(stdout)
// }

// fn heuristic_should_panic(stdout: &str) -> Option<String> {
//     for line in stdout.lines() {
//         if line.contains("note: test did not panic as expected") {
//             // Strip invalid XML characters (e.g. ANSI escapes) if they're around.
//             return Some(Output::new(line).into_string());
//         }
//     }
//     None
// }

// fn heuristic_stack_trace(stderr: &str) -> Option<String> {
//     let panicked_at_match = PANICKED_AT_REGEX.find(stderr)?;
//     // If the previous line starts with "Error: ", grab it as well -- it contains the error with
//     // result-based test failures.
//     let mut start = panicked_at_match.start();
//     let prefix = stderr[..start].trim_end_matches('\n');
//     if let Some(prev_line_start) = prefix.rfind('\n') {
//         if prefix[prev_line_start..].starts_with("\nError:") {
//             start = prev_line_start + 1;
//         }
//     }

//     Some(Output::new(stderr[start..].trim_end()).into_string())
// }

// fn heuristic_error_str(stderr: &str) -> Option<String> {
//     // Starting Rust 1.66, Result-based errors simply print out "Error: ".
//     let error_match = ERROR_REGEX.find(stderr)?;
//     let start = error_match.start();
//     Some(Output::new(stderr[start..].trim_end()).into_string())
// }

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn test_heuristic_extract_description() {
//         let tests: &[(&str, &str)] = &[(
//             "running 1 test
// test test_failure_should_panic - should panic ... FAILED

// failures:

// ---- test_failure_should_panic stdout ----
// note: test did not panic as expected

// failures:
//     test_failure_should_panic

// test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 13 filtered out; finished in 0.00s",
//             "note: test did not panic as expected",
//         )];

//         for (input, output) in tests {
//             assert_eq!(heuristic_should_panic(input).as_deref(), Some(*output));
//         }
//     }

//     #[test]
//     fn test_heuristic_stack_trace() {
//         let tests: &[(&str, &str)] = &[
//             (
//                 "thread 'main' panicked at 'foo', src/lib.rs:1\n",
//                 "thread 'main' panicked at 'foo', src/lib.rs:1",
//             ),
//             (
//                 "foobar\n\
//             thread 'main' panicked at 'foo', src/lib.rs:1\n\n",
//                 "thread 'main' panicked at 'foo', src/lib.rs:1",
//             ),
//             (
//                 r#"
// text: foo
// Error: Custom { kind: InvalidData, error: "this is an error" }
// thread 'test_result_failure' panicked at 'assertion failed: `(left == right)`
//   left: `1`,
//  right: `0`: the test returned a termination value with a non-zero status code (1) which indicates a failure', /rustc/fe5b13d681f25ee6474be29d748c65adcd91f69e/library/test/src/lib.rs:186:5
// note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
//             "#,
//                 r#"Error: Custom { kind: InvalidData, error: "this is an error" }
// thread 'test_result_failure' panicked at 'assertion failed: `(left == right)`
//   left: `1`,
//  right: `0`: the test returned a termination value with a non-zero status code (1) which indicates a failure', /rustc/fe5b13d681f25ee6474be29d748c65adcd91f69e/library/test/src/lib.rs:186:5
// note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace"#,
//             ),
//         ];

//         for (input, output) in tests {
//             assert_eq!(heuristic_stack_trace(input).as_deref(), Some(*output));
//         }
//     }

//     #[test]
//     fn test_heuristic_error_str() {
//         let tests: &[(&str, &str)] = &[(
//             "foobar\nError: \"this is an error\"\n",
//             "Error: \"this is an error\"",
//         )];

//         for (input, output) in tests {
//             assert_eq!(heuristic_error_str(input).as_deref(), Some(*output));
//         }
//     }
// }
