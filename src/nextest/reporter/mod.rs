// Copyright (c) The nextest Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Prints out and aggregates test execution statuses.
//!
//! The main structure in this module is [`TestReporter`].

mod aggregator;
// use crate::{
//     config::NextestProfile,
//     errors::WriteEventError,
//     helpers::write_test_name,
//     list::{TestInstance, TestList},
//     reporter::aggregator::EventAggregator,
//     runner::{
//         AbortStatus, ExecuteStatus, ExecutionDescription, ExecutionResult, ExecutionStatuses,
//         RetryData, RunStats,
//     },
// };
use debug_ignore::DebugIgnore;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
// use nextest_metadata::MismatchReason;
use owo_colors::{OwoColorize, Style};
use serde::Deserialize;
use std::{
    borrow::Cow,
    cmp::Reverse,
    fmt::{self, Write as _},
    io,
    io::{BufWriter, Write},
    time::{Duration, SystemTime},
};

use self::aggregator::{EventAggregator, WriteEventError};

use super::{
    ExecuteStatus, ExecutionDescription, ExecutionResult, MismatchReason, RunStats, TestInstance,
    TestList,
};

/// When to display test output in the reporter.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TestOutputDisplay {
    /// Show output immediately on execution completion.
    ///
    /// This is the default for failing tests.
    Immediate,

    /// Show output immediately, and at the end of a test run.
    ImmediateFinal,

    /// Show output at the end of execution.
    Final,

    /// Never show output.
    Never,
}

impl TestOutputDisplay {
    /// Returns true if test output is shown immediately.
    pub fn is_immediate(self) -> bool {
        match self {
            TestOutputDisplay::Immediate | TestOutputDisplay::ImmediateFinal => true,
            TestOutputDisplay::Final | TestOutputDisplay::Never => false,
        }
    }

    /// Returns true if test output is shown at the end of the run.
    pub fn is_final(self) -> bool {
        match self {
            TestOutputDisplay::Final | TestOutputDisplay::ImmediateFinal => true,
            TestOutputDisplay::Immediate | TestOutputDisplay::Never => false,
        }
    }
}

/// Status level to show in the reporter output.
///
/// Status levels are incremental: each level causes all the statuses listed above it to be output. For example,
/// [`Slow`](Self::Slow) implies [`Retry`](Self::Retry) and [`Fail`](Self::Fail).
#[derive(Copy, Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum StatusLevel {
    /// No output.
    None,

    /// Only output test failures.
    Fail,

    /// Output information about slow tests, and all variants above.
    Slow,

    /// Output passing tests in addition to all variants above.
    Pass,

    /// Output skipped tests in addition to all variants above.
    Skip,

    /// Currently has the same meaning as [`Skip`](Self::Skip).
    All,
}

/// Status level to show at the end of test runs in the reporter output.
///
/// Status levels are incremental.
///
/// This differs from [`StatusLevel`] in one ways:
/// * It has a different ordering: skipped tests are prioritized over passing ones.
#[derive(Copy, Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum FinalStatusLevel {
    /// No output.
    None,

    /// Only output test failures.
    Fail,

    /// Output information about slow tests, and all variants above.
    Slow,

    /// Output skipped tests in addition to all variants above.
    Skip,

    /// Output passing tests in addition to all variants above.
    Pass,

    /// Currently has the same meaning as [`Pass`](Self::Pass).
    All,
}

/// Standard error destination for the reporter.
///
/// This is usually a terminal, but can be an in-memory buffer for tests.
pub enum ReporterOutput<'a> {
    /// Produce output on the (possibly piped) terminal.
    ///
    /// If the terminal isn't piped, produce output to a progress bar.
    Stderr,

    /// Write output to a buffer.
    Buffer(&'a mut dyn io::Write),
}

/// Test reporter builder.
#[derive(Debug, Default)]
pub struct TestReporterBuilder {
    failure_output: Option<TestOutputDisplay>,
    success_output: Option<TestOutputDisplay>,
    status_level: Option<StatusLevel>,
    final_status_level: Option<FinalStatusLevel>,
    // verbose: bool,
    hide_progress_bar: bool,
    imitate_cargo: bool,
}

impl TestReporterBuilder {
    // /// Sets the conditions under which test failures are output.
    // pub fn set_failure_output(&mut self, failure_output: TestOutputDisplay) -> &mut Self {
    //     self.failure_output = Some(failure_output);
    //     self
    // }

    // /// Sets the conditions under which test successes are output.
    // pub fn set_success_output(&mut self, success_output: TestOutputDisplay) -> &mut Self {
    //     self.success_output = Some(success_output);
    //     self
    // }

    // /// Sets the kinds of statuses to output.
    // pub fn set_status_level(&mut self, status_level: StatusLevel) -> &mut Self {
    //     self.status_level = Some(status_level);
    //     self
    // }

    // /// Sets the kinds of statuses to output at the end of the run.
    // pub fn set_final_status_level(&mut self, final_status_level: FinalStatusLevel) -> &mut Self {
    //     self.final_status_level = Some(final_status_level);
    //     self
    // }

    // /// Sets verbose output.
    // pub fn set_verbose(&mut self, verbose: bool) -> &mut Self {
    //     self.verbose = verbose;
    //     self
    // }

    // /// Sets visibility of the progress bar.
    // /// The progress bar is also hidden if `no_capture` is set.
    // pub fn set_hide_progress_bar(&mut self, hide_progress_bar: bool) -> &mut Self {
    //     self.hide_progress_bar = hide_progress_bar;
    //     self
    // }

    /// Whether to imitiate the cargo test output for other tools
    pub fn set_imitate_cargo(&mut self, imitate_cargo: bool) -> &mut Self {
        self.imitate_cargo = imitate_cargo;
        self
    }
}

impl TestReporterBuilder {
    /// Creates a new test reporter.
    pub(crate) fn build<'a>(
        &self,
        test_list: &TestList,
        output: ReporterOutput<'a>,
    ) -> TestReporter<'a> {
        let styles = Box::default();
        let aggregator = EventAggregator::new();

        let status_level = self.status_level.unwrap_or(StatusLevel::Pass);
        let final_status_level = self.final_status_level.unwrap_or(FinalStatusLevel::Slow);

        let force_success_output = self.success_output;
        let force_failure_output = self.failure_output;

        let stderr = match output {
            // ReporterStderr::Terminal if is_ci::uncached() => {
            //     // Some CI environments appear to pretend to be a terminal. Disable the progress bar
            //     // in these environments.
            //     ReporterStderrImpl::TerminalWithoutBar
            // }
            ReporterOutput::Stderr if self.hide_progress_bar => {
                ReporterStderrImpl::StderrWithoutBar
            }
            ReporterOutput::Stderr if self.imitate_cargo => ReporterStderrImpl::ImitateCargo,

            ReporterOutput::Stderr => {
                let progress_bar = ProgressBar::new(test_list.tests.len() as u64);
                // Emulate Cargo's style.
                let test_count_width = format!("{}", test_list.tests.len()).len();
                // Create the template using the width as input. This is a little confusing -- {{foo}}
                // is what's passed into the ProgressBar, while {bar} is inserted by the format!() statement.
                //
                // Note: ideally we'd use the same format as our other duration displays for the elapsed time,
                // but that isn't possible due to https://github.com/console-rs/indicatif/issues/440. Use
                // {{elapsed_precise}} as an OK tradeoff here.
                let template = format!(
                    "{{prefix:>12}} [{{elapsed_precise:>9}}] [{{wide_bar}}] \
                    {{pos:>{test_count_width}}}/{{len:{test_count_width}}}: {{msg}}     "
                );
                progress_bar.set_style(
                    ProgressStyle::default_bar()
                        .progress_chars("=> ")
                        .template(&template)
                        .expect("template is known to be valid"),
                );
                // NOTE: set_draw_target must be called before enable_steady_tick to avoid a
                // spurious extra line from being printed as the draw target changes.
                //
                // This used to be unbuffered, but that option went away from indicatif 0.17.0. The
                // refresh rate is now 20hz so that it's double the steady tick rate.
                progress_bar.set_draw_target(ProgressDrawTarget::stderr_with_hz(20));
                // Enable a steady tick 10 times a second.
                progress_bar.enable_steady_tick(Duration::from_millis(100));
                ReporterStderrImpl::StderrWithBar(progress_bar)
            }
            ReporterOutput::Buffer(buf) => ReporterStderrImpl::Buffer(buf),
        };

        TestReporter {
            inner: TestReporterImpl {
                status_level,
                final_status_level,
                force_success_output,
                force_failure_output,
                styles,
                cancel_status: None,
                final_outputs: DebugIgnore(vec![]),
            },
            stderr,
            metadata_reporter: aggregator,
        }
    }
}

enum ReporterStderrImpl<'a> {
    StderrWithBar(ProgressBar),
    StderrWithoutBar,
    ImitateCargo,
    Buffer(&'a mut dyn std::io::Write),
}

/// Functionality to report test results to stderr and JUnit
pub struct TestReporter<'a> {
    inner: TestReporterImpl,
    stderr: ReporterStderrImpl<'a>,
    metadata_reporter: EventAggregator<'a>,
}

impl<'a> TestReporter<'a> {
    /// Colorizes output.
    pub fn colorize(&mut self) {
        self.inner.styles.colorize();
    }

    /// Report a test event.
    pub(crate) fn report_event(&mut self, event: TestEvent<'a>) -> Result<(), WriteEventError> {
        self.write_event(event)
    }

    // ---
    // Helper methods
    // ---

    /// Report this test event to the given writer.
    fn write_event(&mut self, event: TestEvent<'a>) -> Result<(), WriteEventError> {
        match &mut self.stderr {
            ReporterStderrImpl::StderrWithBar(progress_bar) => {
                // Write to a string that will be printed as a log line.
                let mut buf: Vec<u8> = Vec::new();
                self.inner
                    .write_event_impl(&event, &mut buf)
                    .map_err(WriteEventError::Io)?;
                // ProgressBar::println doesn't print status lines if the bar is hidden. The suspend
                // method prints it in both cases.
                progress_bar.suspend(|| {
                    _ = std::io::stderr().write_all(&buf);
                });

                update_progress_bar(&event, &self.inner.styles, progress_bar);
            }
            ReporterStderrImpl::StderrWithoutBar => {
                // Write to a buffered stderr.
                let mut writer = BufWriter::new(std::io::stderr());
                self.inner
                    .write_event_impl(&event, &mut writer)
                    .map_err(WriteEventError::Io)?;
                writer.flush().map_err(WriteEventError::Io)?;
            }
            ReporterStderrImpl::ImitateCargo => {
                // Write to a buffered stderr.
                let mut stderr = BufWriter::new(std::io::stderr());
                let mut stdout = BufWriter::new(std::io::stdout());

                match &event {
                    TestEvent::RunStarted { .. } => {}
                    TestEvent::TestFinished {
                        test_instance,
                        run_status,
                        ..
                    } => {
                        if run_status.result != ExecutionResult::Pass {
                            self.inner.final_outputs.push((
                                test_instance.clone(),
                                FinalOutput::Executed {
                                    run_status: run_status.clone(),
                                    test_output_display: TestOutputDisplay::Final,
                                },
                            ))
                        }
                        let s = match run_status.result == ExecutionResult::Pass {
                            true => "ok",
                            false => "FAILED",
                        };
                        writeln!(stdout, "test {} ... {s}", test_instance.name,)
                            .map_err(WriteEventError::Io)?;
                    }
                    TestEvent::RunFinished {
                        elapsed, run_stats, ..
                    } => {
                        if !self.inner.final_outputs.is_empty() {
                            writeln!(stdout, "\nfailures:").map_err(WriteEventError::Io)?;
                            stdout.flush().map_err(WriteEventError::Io)?;

                            for (instance, output) in self.inner.final_outputs.iter() {
                                if let FinalOutput::Executed {
                                    run_status:
                                        ExecuteStatus {
                                            output: Some(msg), ..
                                        },
                                    ..
                                } = output
                                {
                                    // writeln!(stderr, "---- {} stdout ----", instance.name)
                                    //     .map_err(WriteEventError::Io)?;
                                    writeln!(stderr, "{msg}\n").map_err(WriteEventError::Io)?;
                                    stderr.flush().map_err(WriteEventError::Io)?;
                                }
                            }

                            writeln!(stdout, "\nfailures:").map_err(WriteEventError::Io)?;
                            for (instance, _) in self.inner.final_outputs.iter() {
                                writeln!(stdout, "    {}", instance.name)
                                    .map_err(WriteEventError::Io)?;
                            }
                        }

                        // let s = match !run_stats.any_failed() {
                        //     true => "ok",
                        //     false => "FAILED",
                        // };
                        // writeln!(stdout, "\ntest result: {s}. {} passed; {} failed; {} ignored; finished in {:.2}s",
                        //     run_stats.passed,
                        //     run_stats.failed,
                        //     run_stats.skipped,
                        //     elapsed.as_secs_f64()
                        // )
                        // .map_err(WriteEventError::Io)?;
                    }
                    TestEvent::TestStarted { .. } => {}
                    TestEvent::TestSlow { .. } => {}
                    TestEvent::TestSkipped { .. } => {}
                    TestEvent::RunBeginCancel { .. } => {}
                    TestEvent::RunPaused { .. } => {}
                    TestEvent::RunContinued { .. } => {}
                }

                stdout.flush().map_err(WriteEventError::Io)?;
                stderr.flush().map_err(WriteEventError::Io)?;
            }
            ReporterStderrImpl::Buffer(buf) => {
                self.inner
                    .write_event_impl(&event, buf)
                    .map_err(WriteEventError::Io)?;
            }
        }
        self.metadata_reporter.write_event(event)?;
        Ok(())
    }
}

fn update_progress_bar(event: &TestEvent<'_>, styles: &Styles, progress_bar: &mut ProgressBar) {
    match event {
        TestEvent::TestStarted {
            current_stats,
            running,
            cancel_state,
            ..
        }
        | TestEvent::TestFinished {
            current_stats,
            running,
            cancel_state,
            ..
        } => {
            let running_state = RunningState::new(*cancel_state, current_stats);
            progress_bar.set_prefix(running_state.progress_bar_prefix(styles));
            progress_bar.set_message(progress_bar_msg(current_stats, *running, styles));
            // If there are skipped tests, the initial run count will be lower than when constructed
            // in ProgressBar::new.
            progress_bar.set_length(current_stats.initial_run_count as u64);
            progress_bar.set_position(current_stats.finished_count as u64);
        }
        TestEvent::RunBeginCancel { reason, .. } => {
            let running_state = RunningState::Canceling(*reason);
            progress_bar.set_prefix(running_state.progress_bar_prefix(styles));
        }
        _ => {}
    }
}

#[derive(Copy, Clone, Debug)]
enum RunningState<'a> {
    Running(&'a RunStats),
    Canceling(CancelReason),
}

impl<'a> RunningState<'a> {
    fn new(cancel_state: Option<CancelReason>, current_stats: &'a RunStats) -> Self {
        match cancel_state {
            Some(cancel_state) => Self::Canceling(cancel_state),
            None => Self::Running(current_stats),
        }
    }

    fn progress_bar_prefix(self, styles: &Styles) -> String {
        let (prefix_str, prefix_style) = match self {
            Self::Running(current_stats) => {
                let prefix_style = if current_stats.any_failed() {
                    styles.fail
                } else {
                    styles.pass
                };
                ("Running", prefix_style)
            }
            Self::Canceling(_) => ("Canceling", styles.fail),
        };

        format!("{:>12}", prefix_str.style(prefix_style))
    }
}

fn progress_bar_msg(current_stats: &RunStats, running: usize, styles: &Styles) -> String {
    let mut s = format!("{} running, ", running.style(styles.count));
    // Writing to strings is infallible.
    let _ = write_summary_str(current_stats, styles, &mut s);
    s
}

fn write_summary_str(run_stats: &RunStats, styles: &Styles, out: &mut String) -> fmt::Result {
    write!(
        out,
        "{} {}",
        run_stats.passed.style(styles.count),
        "passed".style(styles.pass)
    )?;

    if run_stats.passed_slow > 0 {
        let mut text = Vec::with_capacity(3);
        if run_stats.passed_slow > 0 {
            text.push(format!(
                "{} {}",
                run_stats.passed_slow.style(styles.count),
                "slow".style(styles.skip),
            ));
        }
        write!(out, " ({})", text.join(", "))?;
    }
    write!(out, ", ")?;

    if run_stats.failed > 0 {
        write!(
            out,
            "{} {}, ",
            run_stats.failed.style(styles.count),
            "failed".style(styles.fail),
        )?;
    }

    if run_stats.timed_out > 0 {
        write!(
            out,
            "{} {}, ",
            run_stats.timed_out.style(styles.count),
            "timed out".style(styles.fail),
        )?;
    }

    write!(
        out,
        "{} {}",
        run_stats.skipped.style(styles.count),
        "skipped".style(styles.skip),
    )?;

    Ok(())
}

#[derive(Debug)]
enum FinalOutput {
    Skipped(MismatchReason),
    Executed {
        run_status: ExecuteStatus,
        test_output_display: TestOutputDisplay,
    },
}

impl FinalOutput {
    fn final_status_level(&self) -> FinalStatusLevel {
        match self {
            Self::Skipped { .. } => FinalStatusLevel::Skip,
            Self::Executed { run_status, .. } => run_status.describe().final_status_level(),
        }
    }
}

struct TestReporterImpl {
    status_level: StatusLevel,
    force_success_output: Option<TestOutputDisplay>,
    force_failure_output: Option<TestOutputDisplay>,
    // binary_id_width: usize,
    final_status_level: FinalStatusLevel,
    styles: Box<Styles>,
    cancel_status: Option<CancelReason>,
    final_outputs: DebugIgnore<Vec<(TestInstance, FinalOutput)>>,
}

impl<'a> TestReporterImpl {
    fn write_event_impl(
        &mut self,
        event: &TestEvent<'a>,
        writer: &mut impl Write,
    ) -> io::Result<()> {
        match event {
            TestEvent::RunStarted { test_list, .. } => {
                write!(writer, "{:>12} ", "Starting".style(self.styles.pass))?;

                let count_style = self.styles.count;

                let tests_str: &str = if test_list.run_count() == 1 {
                    "test"
                } else {
                    "tests"
                };

                write!(
                    writer,
                    "{} {tests_str}",
                    test_list.run_count().style(count_style),
                )?;

                let skip_count = test_list.skip_count();
                if skip_count > 0 {
                    write!(writer, " ({} skipped)", skip_count.style(count_style))?;
                }

                writeln!(writer)?;
            }
            TestEvent::TestStarted { .. } => {}
            TestEvent::TestSlow {
                test_instance,
                elapsed,
                will_terminate,
            } => {
                if !*will_terminate && self.status_level >= StatusLevel::Slow {
                    write!(writer, "{:>12} ", "SLOW".style(self.styles.skip))?;
                } else if *will_terminate {
                    let (_required_status_level, style) = (StatusLevel::Fail, self.styles.fail);
                    write!(writer, "{:>12} ", "TERMINATING".style(style))?;
                }

                self.write_slow_duration(*elapsed, writer)?;
                self.write_instance(test_instance, writer)?;
                writeln!(writer)?;
            }

            TestEvent::TestFinished {
                test_instance,
                success_output,
                failure_output,
                run_status,
                ..
            } => {
                let describe = run_status.describe();
                let last_status = run_status.result;
                let test_output_display = match last_status == ExecutionResult::Pass {
                    true => self.success_output(*success_output),
                    false => self.failure_output(*failure_output),
                };

                if self.status_level >= describe.status_level() {
                    self.write_status_line(test_instance, describe, writer)?;

                    // If the test failed to execute, print its output and error status.
                    // (don't print out test failures after Ctrl-C)
                    if self.cancel_status < Some(CancelReason::Signal)
                        && test_output_display.is_immediate()
                    {
                        self.write_stdout_stderr(test_instance, run_status, false, writer)?;
                    }
                }

                // Store the output in final_outputs if test output display is requested, or if
                // we have to print a one-line summary at the end.
                if test_output_display.is_final()
                    || self.final_status_level >= describe.final_status_level()
                {
                    self.final_outputs.push((
                        test_instance.clone(),
                        FinalOutput::Executed {
                            run_status: run_status.clone(),
                            test_output_display,
                        },
                    ));
                }
            }
            TestEvent::TestSkipped {
                test_instance,
                reason,
            } => {
                if self.status_level >= StatusLevel::Skip {
                    self.write_skip_line(test_instance, writer)?;
                }
                if self.final_status_level >= FinalStatusLevel::Skip {
                    self.final_outputs
                        .push((test_instance.clone(), FinalOutput::Skipped(*reason)));
                }
            }
            TestEvent::RunBeginCancel { running, reason } => {
                self.cancel_status = self.cancel_status.max(Some(*reason));

                write!(writer, "{:>12} ", "Canceling".style(self.styles.fail))?;
                let reason_str = match reason {
                    // CancelReason::TestFailure => "test failure",
                    // CancelReason::ReportError => "error",
                    CancelReason::Signal => "signal",
                    // CancelReason::Interrupt => "interrupt",
                };

                writeln!(
                    writer,
                    "due to {}: {} tests still running",
                    reason_str.style(self.styles.fail),
                    running.style(self.styles.count)
                )?;
            }
            TestEvent::RunPaused { running } => {
                writeln!(
                    writer,
                    "{:>12} {} running tests due to {}",
                    "Pausing".style(self.styles.pass),
                    running.style(self.styles.count),
                    "signal".style(self.styles.count),
                )?;
            }
            TestEvent::RunContinued { running } => {
                writeln!(
                    writer,
                    "{:>12} {} running tests due to {}",
                    "Continuing".style(self.styles.pass),
                    running.style(self.styles.count),
                    "signal".style(self.styles.count),
                )?;
            }
            TestEvent::RunFinished {
                start_time: _start_time,
                elapsed,
                run_stats,
                ..
            } => {
                let summary_style = if run_stats.any_failed() {
                    self.styles.fail
                } else {
                    self.styles.pass
                };
                write!(
                    writer,
                    "------------\n{:>12} ",
                    "Summary".style(summary_style)
                )?;

                // Next, print the total time taken.
                // * > means right-align.
                // * 8 is the number of characters to pad to.
                // * .3 means print two digits after the decimal point.
                write!(writer, "[{:>8.3?}s] ", elapsed.as_secs_f64())?;

                write!(
                    writer,
                    "{}",
                    run_stats.finished_count.style(self.styles.count)
                )?;
                if run_stats.finished_count != run_stats.initial_run_count {
                    write!(
                        writer,
                        "/{}",
                        run_stats.initial_run_count.style(self.styles.count)
                    )?;
                }

                let tests_str = if run_stats.finished_count == 1 && run_stats.initial_run_count == 1
                {
                    "test"
                } else {
                    "tests"
                };

                let mut summary_str = String::new();
                // Writing to a string is infallible.
                let _ = write_summary_str(run_stats, &self.styles, &mut summary_str);
                writeln!(writer, " {tests_str} run: {summary_str}")?;

                // // Don't print out final outputs if canceled due to Ctrl-C.
                // if self.cancel_status < Some(CancelReason::Signal) {
                // Sort the final outputs for a friendlier experience.
                self.final_outputs
                    .sort_by_key(|(test_instance, final_output)| {
                        // Use the final status level, reversed (i.e. failing tests are printed at the very end).
                        (
                            Reverse(final_output.final_status_level()),
                            test_instance.name.clone(),
                        )
                    });

                for (test_instance, final_output) in &*self.final_outputs {
                    let final_status_level = final_output.final_status_level();
                    match final_output {
                        FinalOutput::Skipped(_) => {
                            self.write_skip_line(test_instance, writer)?;
                        }
                        FinalOutput::Executed {
                            run_status,
                            test_output_display,
                        } => {
                            let _last_status = run_status.result;

                            // Print out the final status line so that status lines are shown
                            // for tests that e.g. failed due to signals.
                            if self.final_status_level >= final_status_level
                                || test_output_display.is_final()
                            {
                                self.write_final_status_line(
                                    test_instance,
                                    run_status.describe(),
                                    writer,
                                )?;
                            }
                            if test_output_display.is_final() {
                                self.write_stdout_stderr(test_instance, run_status, false, writer)?;
                            }
                        }
                    }
                }
                // }
            }
        }

        Ok(())
    }

    fn write_skip_line(
        &self,
        test_instance: &TestInstance,
        writer: &mut impl Write,
    ) -> io::Result<()> {
        write!(writer, "{:>12} ", "SKIP".style(self.styles.skip))?;
        // same spacing [   0.034s]
        write!(writer, "[         ] ")?;

        self.write_instance(test_instance, writer)?;
        writeln!(writer)?;

        Ok(())
    }

    fn write_status_line(
        &self,
        test_instance: &TestInstance,
        describe: ExecutionDescription<'_>,
        writer: &mut impl Write,
    ) -> io::Result<()> {
        let status = match describe {
            ExecutionDescription::Success { status } => {
                write!(writer, "{:>12} ", "PASS".style(self.styles.pass))?;
                status
            }
            ExecutionDescription::Failure { status } => {
                write!(
                    writer,
                    "{:>12} ",
                    status_str(status.result).style(self.styles.fail)
                )?;
                status
            }
        };

        // Next, print the time taken.
        self.write_duration(status.time_taken, writer)?;

        // Print the name of the test.
        self.write_instance(test_instance, writer)?;
        writeln!(writer)?;

        Ok(())
    }

    fn write_final_status_line(
        &self,
        test_instance: &TestInstance,
        describe: ExecutionDescription<'_>,
        writer: &mut impl Write,
    ) -> io::Result<()> {
        let status = match describe {
            ExecutionDescription::Success { status } => match (status.is_slow, status.result) {
                (true, _) => {
                    write!(writer, "{:>12} ", "SLOW".style(self.styles.skip))?;
                    status
                }
                (false, _) => {
                    write!(writer, "{:>12} ", "PASS".style(self.styles.pass))?;
                    status
                }
            },
            ExecutionDescription::Failure { status } => {
                write!(
                    writer,
                    "{:>12} ",
                    status_str(status.result).style(self.styles.fail)
                )?;
                status
            }
        };

        // Next, print the time taken.
        self.write_duration(status.time_taken, writer)?;

        // Print the name of the test.
        self.write_instance(test_instance, writer)?;
        writeln!(writer)?;

        Ok(())
    }

    fn write_instance(&self, instance: &TestInstance, writer: &mut impl Write) -> io::Result<()> {
        write!(writer, "{:>width$} ", "test_suite", width = 4)?;

        write_test_name(&instance.name, &self.styles.list_styles, writer)
    }

    fn write_duration(&self, duration: Duration, writer: &mut impl Write) -> io::Result<()> {
        // * > means right-align.
        // * 8 is the number of characters to pad to.
        // * .3 means print three digits after the decimal point.
        write!(writer, "[{:>8.3?}s] ", duration.as_secs_f64())
    }

    // fn write_duration_by(&self, duration: Duration, writer: &mut impl Write) -> io::Result<()> {
    //     // * > means right-align.
    //     // * 7 is the number of characters to pad to.
    //     // * .3 means print three digits after the decimal point.
    //     write!(writer, "by {:>7.3?}s ", duration.as_secs_f64())
    // }

    fn write_slow_duration(&self, duration: Duration, writer: &mut impl Write) -> io::Result<()> {
        // Inside the curly braces:
        // * > means right-align.
        // * 7 is the number of characters to pad to.
        // * .3 means print three digits after the decimal point.
        write!(writer, "[>{:>7.3?}s] ", duration.as_secs_f64())
    }

    fn write_stdout_stderr(
        &self,
        test_instance: &TestInstance,
        run_status: &ExecuteStatus,
        is_retry: bool,
        writer: &mut impl Write,
    ) -> io::Result<()> {
        let (header_style, _output_style) = if is_retry {
            (self.styles.retry, self.styles.retry_output)
        } else if run_status.result == ExecutionResult::Pass {
            (self.styles.pass, self.styles.pass_output)
        } else {
            (self.styles.fail, self.styles.fail_output)
        };

        if let Some(output) = &run_status.output {
            write!(writer, "\n{}", "--- ".style(header_style))?;
            // let out_len = self.write_attempt(run_status, header_style, writer)?;
            // The width is to align test instances.
            write!(
                writer,
                "{:width$}",
                "STDOUT:".style(header_style),
                width = 21
            )?;
            self.write_instance(test_instance, writer)?;
            writeln!(writer, "{}", " ---".style(header_style))?;

            self.write_test_output(output.as_bytes(), writer)?;
        }

        // if !run_status.stderr.is_empty() {
        //     write!(writer, "\n{}", "--- ".style(header_style))?;
        //     let out_len = self.write_attempt(run_status, header_style, writer)?;
        //     // The width is to align test instances.
        //     write!(
        //         writer,
        //         "{:width$}",
        //         "STDERR:".style(header_style),
        //         width = (21 - out_len)
        //     )?;
        //     self.write_instance(test_instance.clone(), writer)?;
        //     writeln!(writer, "{}", " ---".style(header_style))?;

        //     self.write_test_output(&run_status.stderr, writer)?;
        // }

        writeln!(writer)
    }

    fn write_test_output(&self, output: &[u8], writer: &mut impl Write) -> io::Result<()> {
        if self.styles.is_colorized {
            const RESET_COLOR: &[u8] = b"\x1b[0m";
            // Output the text without stripping ANSI escapes, then reset the color afterwards in case
            // the output is malformed.
            writer.write_all(output)?;
            writer.write_all(RESET_COLOR)?;
        } else {
            // // Strip ANSI escapes from the output if nextest itself isn't colorized.
            // let mut no_color = strip_ansi_escapes::Writer::new(writer);
            // no_color.write_all(output)?;
        }

        Ok(())
    }

    // // Returns the number of characters written out to the screen.
    // fn write_attempt(
    //     &self,
    //     run_status: &ExecuteStatus,
    //     style: Style,
    //     writer: &mut impl Write,
    // ) -> io::Result<usize> {
    //     if run_status.retry_data.total_attempts > 1 {
    //         // 3 for 'TRY' + 1 for ' ' + length of the current attempt + 1 for following space.
    //         let attempt_str = format!("{}", run_status.retry_data.attempt);
    //         let out_len = 3 + 1 + attempt_str.len() + 1;
    //         write!(
    //             writer,
    //             "{} {} ",
    //             "TRY".style(style),
    //             attempt_str.style(style)
    //         )?;
    //         Ok(out_len)
    //     } else {
    //         Ok(0)
    //     }
    // }

    fn success_output(&self, test_setting: TestOutputDisplay) -> TestOutputDisplay {
        self.force_success_output.unwrap_or(test_setting)
    }

    fn failure_output(&self, test_setting: TestOutputDisplay) -> TestOutputDisplay {
        self.force_failure_output.unwrap_or(test_setting)
    }
}

impl<'a> fmt::Debug for TestReporter<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("TestReporter")
            .field("stdout", &"BufferWriter { .. }")
            .field("stderr", &"BufferWriter { .. }")
            .finish()
    }
}

/// Write out a test name.
fn write_test_name(name: &str, style: &ListStyles, mut writer: impl Write) -> io::Result<()> {
    // Look for the part of the test after the last ::, if any.
    let mut splits = name.rsplitn(2, "::");
    let trailing = splits.next().expect("test should have at least 1 element");
    if let Some(rest) = splits.next() {
        write!(
            writer,
            "{}{}",
            rest.style(style.module_path),
            "::".style(style.module_path)
        )?;
    }
    write!(writer, "{}", trailing.style(style.test_name))?;

    Ok(())
}

fn status_str(result: ExecutionResult) -> Cow<'static, str> {
    // Max 12 characters here.
    match result {
        ExecutionResult::Fail => "FAIL".into(),
        ExecutionResult::Pass => "PASS".into(),
        ExecutionResult::Timeout => "TIMEOUT".into(),
    }
}

// fn short_status_str(result: ExecutionResult) -> Cow<'static, str> {
//     // Use shorter strings for this (max 6 characters).
//     match result {
//         ExecutionResult::Fail => "FAIL".into(),
//         ExecutionResult::Pass => "PASS".into(),
//         ExecutionResult::Timeout => "TMT".into(),
//     }
// }

/// A test event.
///
/// Events are produced by a [`TestRunner`](crate::runner::TestRunner) and consumed by a [`TestReporter`].
#[derive(Clone, Debug)]
pub(crate) enum TestEvent<'a> {
    /// The test run started.
    RunStarted {
        /// The list of tests that will be run.
        ///
        /// The methods on the test list indicate the number of tests that will be run.
        test_list: &'a TestList,
        // /// The UUID for this run.
        // run_id: Uuid,
    },

    /// A test started running.
    TestStarted {
        // /// The test instance that was started.
        // test_instance: TestInstance,
        /// Current run statistics so far.
        current_stats: RunStats,

        /// The number of tests currently running, including this one.
        running: usize,

        /// The cancel status of the run. This is None if the run is still ongoing.
        cancel_state: Option<CancelReason>,
    },

    /// A test was slower than a configured soft timeout.
    TestSlow {
        /// The test instance that was slow.
        test_instance: TestInstance,

        // /// Retry data.
        // retry_data: RetryData,
        /// The amount of time that has elapsed since the beginning of the test.
        elapsed: Duration,

        /// True if the test has hit its timeout and is about to be terminated.
        will_terminate: bool,
    },

    /// A test finished running.
    TestFinished {
        /// The test instance that finished running.
        test_instance: TestInstance,

        /// Test setting for success output.
        success_output: TestOutputDisplay,

        /// Test setting for failure output.
        failure_output: TestOutputDisplay,

        /// Whether the JUnit report should store success output for this test.
        junit_store_success_output: bool,

        /// Whether the JUnit report should store failure output for this test.
        junit_store_failure_output: bool,

        /// Information about all the runs for this test.
        run_status: ExecuteStatus,

        /// Current statistics for number of tests so far.
        current_stats: RunStats,

        /// The number of tests that are currently running, excluding this one.
        running: usize,

        /// The cancel status of the run. This is None if the run is still ongoing.
        cancel_state: Option<CancelReason>,
    },

    /// A test was skipped.
    TestSkipped {
        /// The test instance that was skipped.
        test_instance: TestInstance,
        /// The reason this test was skipped.
        reason: MismatchReason,
    },

    /// A cancellation notice was received.
    RunBeginCancel {
        /// The number of tests still running.
        running: usize,

        /// The reason this run was canceled.
        reason: CancelReason,
    },

    /// A SIGTSTP event was received and the run was paused.
    RunPaused {
        /// The number of tests currently running.
        running: usize,
    },

    /// A SIGCONT event was received and the run is being continued.
    RunContinued {
        /// The number of tests that will be started up again.
        running: usize,
    },

    /// The test run finished.
    RunFinished {
        // /// The unique ID for this run.
        // run_id: Uuid,
        /// The time at which the run was started.
        start_time: SystemTime,

        /// The amount of time it took for the tests to run.
        elapsed: Duration,

        /// Statistics for the run.
        run_stats: RunStats,
    },
}

// Note: the order here matters -- it indicates severity of cancellation
/// The reason why a test run is being cancelled.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum CancelReason {
    // /// A test failed and --no-fail-fast wasn't specified.
    // TestFailure,

    // /// An error occurred while reporting results.
    // ReportError,

    /// A termination signal (on Unix, SIGTERM or SIGHUP) was received.
    Signal,

    // /// An interrupt (on Unix, Ctrl-C) was received.
    // Interrupt,
}

#[derive(Debug, Default)]
struct Styles {
    is_colorized: bool,
    count: Style,
    pass: Style,
    retry: Style,
    fail: Style,
    pass_output: Style,
    retry_output: Style,
    fail_output: Style,
    skip: Style,
    list_styles: ListStyles,
}

impl Styles {
    fn colorize(&mut self) {
        self.is_colorized = true;
        self.count = Style::new().bold();
        self.pass = Style::new().green().bold();
        self.retry = Style::new().magenta().bold();
        self.fail = Style::new().red().bold();
        self.pass_output = Style::new().green();
        self.retry_output = Style::new().magenta();
        self.fail_output = Style::new().magenta();
        self.skip = Style::new().yellow().bold();
        self.list_styles.colorize();
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ListStyles {
    pub(crate) binary_id: Style,
    pub(crate) test_name: Style,
    pub(crate) module_path: Style,
    pub(crate) field: Style,
}

impl ListStyles {
    pub(crate) fn colorize(&mut self) {
        self.binary_id = Style::new().magenta().bold();
        self.test_name = Style::new().blue().bold();
        self.field = Style::new().yellow().bold();
        self.module_path = Style::new().cyan();
    }
}
