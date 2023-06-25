//! Write your own tests and benchmarks that look and behave like built-in tests!
//!
//! This is a simple and small test harness that mimics the original `libtest`
//! (used by `cargo test`/`rustc --test`). That means: all output looks pretty
//! much like `cargo test` and most CLI arguments are understood and used. With
//! that plumbing work out of the way, your test runner can focus on the actual
//! testing.
//!
//! For a small real world example, see [`examples/tidy.rs`][1].
//!
//! [1]: https://github.com/conradludgate/async-test/blob/master/examples/tidy.rs
//!
//! # Usage
//!
//! To use this, you most likely want to add a manual `[[test]]` section to
//! `Cargo.toml` and set `harness = false`. For example:
//!
//! ```toml
//! [[test]]
//! name = "mytest"
//! path = "tests/mytest.rs"
//! harness = false
//! ```
//!
//! And in `tests/mytest.rs` you would call [`main`] in the `main` function:
//!
//! ```no_run
//! // Create a list of tests (in this case: two dummy tests).
//! async_test::test!(
//!     async fn succeeding_test() {}
//! );
//!
//! async_test::test!(
//!     async fn failing_test() { panic!("Woops") }
//! );
//!
//! // Run all tests and exit the application appropriatly.
//! async_test::main();
//! ```
//!
//! Instead of returning `Ok` or `Err` directly, you want to actually perform
//! your tests, of course. See [`Trial::test`] for more information on how to
//! define a test. You can of course list all your tests manually. But in many
//! cases it is useful to generate one test per file in a directory, for
//! example.
//!
//! You can then run `cargo test --test mytest` to run it. To see the CLI
//! arguments supported by this crate, run `cargo test --test mytest -- -h`.
//!
//!
//! # Known limitations and differences to the official test harness
//!
//! `async-test` works on a best-effort basis: it tries to be as close to
//! `libtest` as possible, but there are differences for a variety of reasons.
//! For example, some rarely used features might not be implemented, some
//! features are extremely difficult to implement, and removing minor,
//! unimportant differences is just not worth the hassle.
//!
//! Some of the notable differences:
//!
//! - Output capture and `--nocapture`: simply not supported. The official
//!   `libtest` uses internal `std` functions to temporarily redirect output.
//!   `async-test` cannot use those.
//! - `--format=json|junit`

#![forbid(unsafe_code)]

mod nextest;

use std::{
    any::TypeId,
    collections::HashSet,
    fmt,
    future::Future,
    hash::Hash,
    num::NonZeroUsize,
    os::macos::raw::stat,
    pin::Pin,
    process,
    sync::{Arc, Mutex, OnceLock},
    task::Poll,
    time::{Duration, SystemTime},
};

mod args;
mod printer;

use nextest::{
    reporter::{ReporterStderr, TestEvent, TestReporter, TestReporterBuilder},
    ExecuteStatus, RunStats, TestInstance, TestList,
};
use printer::Printer;
use tokio::{sync::Semaphore, task::JoinSet};
use uuid::Uuid;

pub use crate::args::{Arguments, ColorSetting, FormatSetting};

/// A single test or benchmark.
///
/// The original `libtest` often calls benchmarks "tests", which is a bit
/// confusing. So in this library, it is called "trial".
///
/// A trial is created via [`Trial::test`] or [`Trial::bench`]. The trial's
/// `name` is printed and used for filtering. The `runner` is called when the
/// test/benchmark is executed to determine its outcome. If `runner` panics,
/// the trial is considered "failed". If you need the behavior of
/// `#[should_panic]` you need to catch the panic yourself. You likely want to
/// compare the panic payload to an expected value anyway.
pub struct Trial {
    runner: Box<dyn FnOnce() -> Box<dyn Future<Output = ()> + Send> + Send>,
    info: TestInfo,
}

impl Trial {
    /// Creates a (non-benchmark) test with the given name and runner.
    ///
    /// The runner returning `Ok(())` is interpreted as the test passing. If the
    /// runner returns `Err(_)`, the test is considered failed.
    pub fn test<R, F>(name: impl Into<String>, runner: R) -> Self
    where
        R: FnOnce() -> F + Send + 'static,
        F: Future<Output = ()> + Send + 'static,
    {
        Self {
            runner: Box::new(move || Box::new(runner())),
            info: TestInfo {
                name: name.into(),
                kind: String::new(),
                is_ignored: false,
            },
        }
    }

    /// Sets the "kind" of this test/benchmark. If this string is not
    /// empty, it is printed in brackets before the test name (e.g.
    /// `test [my-kind] test_name`). (Default: *empty*)
    ///
    /// This is the only extension to the original libtest.
    pub fn with_kind(self, kind: impl Into<String>) -> Self {
        Self {
            info: TestInfo {
                kind: kind.into(),
                ..self.info
            },
            ..self
        }
    }

    /// Sets whether or not this test is considered "ignored". (Default: `false`)
    ///
    /// With the built-in test suite, you can annotate `#[ignore]` on tests to
    /// not execute them by default (for example because they take a long time
    /// or require a special environment). If the `--ignored` flag is set,
    /// ignored tests are executed, too.
    pub fn with_ignored_flag(self, is_ignored: bool) -> Self {
        Self {
            info: TestInfo {
                is_ignored,
                ..self.info
            },
            ..self
        }
    }

    /// Returns the name of this trial.
    pub fn name(&self) -> &str {
        &self.info.name
    }

    /// Returns the kind of this trial. If you have not set a kind, this is an
    /// empty string.
    pub fn kind(&self) -> &str {
        &self.info.kind
    }

    /// Returns whether this trial has been marked as *ignored*.
    pub fn has_ignored_flag(&self) -> bool {
        self.info.is_ignored
    }
}

impl fmt::Debug for Trial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct OpaqueRunner;
        impl fmt::Debug for OpaqueRunner {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("<runner>")
            }
        }

        f.debug_struct("Test")
            .field("runner", &OpaqueRunner)
            .field("name", &self.info.name)
            .field("kind", &self.info.kind)
            .field("is_ignored", &self.info.is_ignored)
            .finish()
    }
}

// struct Config {}

// type AnyOwnedVal = Box<dyn std::any::Any + Send + Sync + 'static>;
// type AnySharedVal = Arc<dyn std::any::Any + Send + Sync + 'static>;

struct Setup {
    type_id: fn() -> TypeId,
    module: &'static str,
    // function: &'static str,
    // file: &'static str,
    // line: u32,
    // setup: tokio::sync::Mutex<MaybeSetup>,
}
struct SetupInit {
    type_id: fn() -> TypeId,
    module: &'static str,
    // function: &'static str,
    // file: &'static str,
    // line: u32,
    // setup: MaybeSetupInit,
}

impl Hash for Setup {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.type_id.hash(state);
        self.module.hash(state);
    }
}
impl PartialEq for Setup {
    fn eq(&self, other: &Self) -> bool {
        self.type_id == other.type_id && self.module == other.module
    }
}
impl Eq for Setup {}

// enum MaybeSetupInit {
//     // /// should be called many times
//     // Ephemeral(fn() -> tokio::task::JoinHandle<AnyOwnedVal>),
//     /// should be called once
//     SharedInit(fn() -> tokio::task::JoinHandle<AnySharedVal>),
// }
// enum MaybeSetup {
//     /// should be called many times
//     Ephemeral(fn() -> tokio::task::JoinHandle<AnyOwnedVal>),
//     /// should be called once
//     SharedInit(fn() -> tokio::task::JoinHandle<AnySharedVal>),
//     /// called once and waiting
//     Running(tokio::task::JoinHandle<AnySharedVal>),
//     /// done
//     Done(AnySharedVal),
// }

inventory::collect!(SetupInit);

// fn setup_config() -> tokio::task::JoinHandle<AnySharedVal> {
//     inventory::submit! {
//         SetupInit {
//             type_id: TypeId::of::<Config>,
//             module: module_path!(),
//             // function: "setup_config",
//             // file: file!(),
//             // line: line!()
//             // setup: MaybeSetupInit::SharedInit(setup_config),
//         }
//     }

//     async fn __inner() -> Config {
//         Config {}
//     }
//     tokio::spawn(async { Arc::new(__inner().await) as Arc<_> })
// }

struct Context {
    values: HashSet<Arc<Setup>>,
}

#[derive(Clone)]
pub struct Tester {
    inner: Arc<Mutex<TesterInner>>,
}

impl Tester {
    pub fn add(&self, trial: Trial) {
        self.inner.lock().unwrap().tasks.push(trial)
    }
}

struct TesterInner {
    tasks: Vec<Trial>,
}

mod builder {
    use crate::Tester;

    pub struct TestBuilder(pub fn(tester: Tester));
    inventory::collect!(TestBuilder);
}

// inventory::submit! {TestBuilder(foo)}
// fn foo(mut tester: Tester) {}

fn setup_tests() -> (Tester, Context) {
    let mut context = Context {
        values: HashSet::new(),
    };
    for setup in inventory::iter::<SetupInit>() {
        context.values.insert(Arc::new(Setup {
            type_id: setup.type_id,
            module: setup.module,
            // function: setup.function,
            // file: setup.file,
            // line: setup.line,
            // setup: tokio::sync::Mutex::new(match setup.setup {
            //     MaybeSetupInit::Ephemeral(f) => MaybeSetup::Ephemeral(f),
            //     MaybeSetupInit::SharedInit(f) => MaybeSetup::SharedInit(f),
            // }),
        }));
    }
    let tester = Tester {
        inner: Arc::new(Mutex::new(TesterInner { tasks: vec![] })),
    };
    for builder in inventory::iter::<builder::TestBuilder>() {
        (builder.0)(tester.clone())
    }
    (tester, context)
}

#[derive(Debug, Clone)]
pub(crate) struct TestInfo {
    name: String,
    kind: String,
    is_ignored: bool,
}

/// The outcome of performing a test/benchmark.
#[derive(Debug, Clone)]
enum Outcome {
    /// The test passed.
    Passed,

    /// The test failed.
    Failed(String),

    /// The test was ignored.
    Ignored,
}

/// Contains information about the entire test run. Is returned by [`run`].
///
/// This type is marked as `#[must_use]`. Usually, you just call
/// [`exit()`][Conclusion::exit] on the result of `run` to exit the application
/// with the correct exit code. But you can also store this value and inspect
/// its data.
#[derive(Clone, Debug, PartialEq, Eq)]
#[must_use = "Call `exit()` or `exit_if_failed()` to set the correct return code"]
pub struct Conclusion {
    /// Number of tests and benchmarks that were filtered out (either by the
    /// filter-in pattern or by `--skip` arguments).
    pub num_filtered_out: u64,

    /// Number of passed tests.
    pub num_passed: u64,

    /// Number of failed tests and benchmarks.
    pub num_failed: u64,

    /// Number of ignored tests and benchmarks.
    pub num_ignored: u64,
}

impl Conclusion {
    /// Exits the application with an appropriate error code (0 if all tests
    /// have passed, 101 if there have been failures).
    pub fn exit(&self) -> ! {
        self.exit_if_failed();
        process::exit(0);
    }

    /// Exits the application with error code 101 if there were any failures.
    /// Otherwise, returns normally.
    pub fn exit_if_failed(&self) {
        if self.has_failed() {
            process::exit(101)
        }
    }

    /// Returns whether there have been any failures.
    pub fn has_failed(&self) -> bool {
        self.num_failed > 0
    }

    fn empty() -> Self {
        Self {
            num_filtered_out: 0,
            num_passed: 0,
            num_failed: 0,
            num_ignored: 0,
        }
    }
}

impl Arguments {
    /// Returns `true` if the given test should be ignored.
    fn is_ignored(&self, test: &Trial) -> bool {
        test.info.is_ignored && !self.ignored && !self.include_ignored
    }

    fn is_filtered_out(&self, test: &Trial) -> bool {
        let test_name = &test.info.name;

        // If a filter was specified, apply this
        if let Some(filter) = &self.filter {
            match self.exact {
                true if test_name != filter => return true,
                false if !test_name.contains(filter) => return true,
                _ => {}
            };
        }

        // If any skip pattern were specified, test for all patterns.
        for skip_filter in &self.skip {
            match self.exact {
                true if test_name == skip_filter => return true,
                false if test_name.contains(skip_filter) => return true,
                _ => {}
            }
        }

        if self.ignored && !test.info.is_ignored {
            return true;
        }

        false
    }
}

/// Runs all given tests.
///
/// This is the central function of this crate. It provides the framework for
/// the testing harness. It does all the printing and house keeping.
pub fn main() {
    let args = Arguments::from_args();
    run(&args).exit_if_failed();
}

/// Runs all given tests.
///
/// This is the central function of this crate. It provides the framework for
/// the testing harness. It does all the printing and house keeping.
///
/// The returned value contains a couple of useful information. See
/// [`Conclusion`] for more information. If `--list` was specified, a list is
/// printed and a dummy `Conclusion` is returned.
pub fn run(args: &Arguments) -> Conclusion {
    let start_instant = SystemTime::now();

    let (tester, _context) = setup_tests();

    // If `--list` is specified, just print the list and return.
    if args.list {
        let mut tests = tester.inner.lock().unwrap();
        if args.filter.is_some() || !args.skip.is_empty() || args.ignored {
            tests.tasks.retain(|test| !args.is_filtered_out(test));
        }

        let mut printer = printer::Printer::new(args, &tests.tasks);
        printer.print_list(&tests.tasks, args.ignored);
        return Conclusion::empty();
    }
    if args.nextest {
        return run_nextest(args, start_instant, tester);
    }

    let mut tests = tester.inner.lock().unwrap();
    let mut conclusion = Conclusion::empty();

    // Apply filtering
    if args.filter.is_some() || !args.skip.is_empty() || args.ignored {
        let len_before = tests.tasks.len() as u64;
        tests.tasks.retain(|test| !args.is_filtered_out(test));
        conclusion.num_filtered_out = len_before - tests.tasks.len() as u64;
    }

    // Create printer which is used for all output.
    let mut printer = printer::Printer::new(args, &tests.tasks);

    // If `--list` is specified, just print the list and return.
    if args.list {
        printer.print_list(&tests.tasks, args.ignored);
        return Conclusion::empty();
    }

    // Print number of tests
    printer.print_title(tests.tasks.len() as u64);

    let mut failed_tests = Vec::new();
    let mut handle_outcome = |outcome: Outcome, test: TestInfo, printer: &mut Printer| {
        printer.print_single_outcome(&outcome);

        // Handle outcome
        match outcome {
            Outcome::Passed => conclusion.num_passed += 1,
            Outcome::Failed(failed) => {
                failed_tests.push((test, failed));
                conclusion.num_failed += 1;
            }
            Outcome::Ignored => conclusion.num_ignored += 1,
        }
    };

    let mut set = JoinSet::new();

    let threads = match args.test_threads.and_then(NonZeroUsize::new) {
        None => std::thread::available_parallelism().unwrap_or(NonZeroUsize::new(1).unwrap()),
        Some(num_threads) => num_threads,
    };

    let mut runtime;

    match threads.get() {
        1 => runtime = tokio::runtime::Builder::new_current_thread(),
        num_threads => {
            runtime = tokio::runtime::Builder::new_multi_thread();
            runtime.worker_threads(num_threads - 1);
        }
    };

    runtime.enable_all();
    let runtime = runtime.build().unwrap();

    let tasks = match args.test_tasks.and_then(NonZeroUsize::new) {
        Some(tasks) => tasks,
        None => threads,
    };

    for test in tests.tasks.drain(..) {
        if set.len() >= tasks.get() {
            let (outcome, test_info, _) = runtime
                .block_on(set.join_next())
                .expect("join set should contain at least 1 test")
                .expect("all test panics should be caught");

            if tasks.get() > 1 {
                printer.print_test(&test_info);
            }
            handle_outcome(outcome, test_info, &mut printer);
        }
        if args.is_ignored(&test) {
            printer.print_test(&test.info);
            handle_outcome(Outcome::Ignored, test.info, &mut printer);
        } else {
            // In multithreaded mode, we do only print the start of the line
            // after the test ran, as otherwise it would lead to terribly
            // interleaved output.
            if tasks.get() == 1 {
                printer.print_test(&test.info);
            }
            set.spawn_on(run_single(test.runner, test.info), runtime.handle());
        }
    }

    while let Some(res) = runtime.block_on(set.join_next()) {
        let (outcome, test_info, _) = res.expect("all test panics should be caught");
        if tasks.get() > 1 {
            printer.print_test(&test_info);
        }
        handle_outcome(outcome, test_info, &mut printer);
    }

    // Print failures if there were any, and the final summary.
    if !failed_tests.is_empty() {
        printer.print_failures(&failed_tests);
    }

    printer.print_summary(&conclusion, start_instant.elapsed().unwrap());

    conclusion
}

fn run_nextest(args: &Arguments, start_instant: SystemTime, tester: Tester) -> Conclusion {
    let mut tests = tester.inner.lock().unwrap();
    let run_id = Uuid::new_v4();

    let test_list = TestList {
        tests: tests.tasks.iter().map(|x| x.info.clone()).collect(),
        skip_count: OnceLock::new(),
    };

    let mut output = args
        .logfile
        .as_deref()
        .map(|f| std::fs::File::create(f).unwrap());
    let report_output = match &mut output {
        Some(file) => ReporterStderr::Buffer(file),
        None => ReporterStderr::Terminal,
    };

    let mut reporter = TestReporterBuilder::default().build(&test_list, report_output);

    match args.color.unwrap_or(ColorSetting::Auto) {
        ColorSetting::Auto => match args.logfile.is_some() {
            true => {}
            false => {
                if supports_color::on(supports_color::Stream::Stderr).map_or(false, |x| x.has_basic)
                {
                    reporter.colorize();
                }
            }
        },
        ColorSetting::Always => reporter.colorize(),
        ColorSetting::Never => {}
    }

    let conclusion = Conclusion::empty();

    let threads = match args.test_threads.and_then(NonZeroUsize::new) {
        None => std::thread::available_parallelism().unwrap_or(NonZeroUsize::new(1).unwrap()),
        Some(num_threads) => num_threads,
    };

    let mut runtime;

    match threads.get() {
        1 => runtime = tokio::runtime::Builder::new_current_thread(),
        num_threads => {
            runtime = tokio::runtime::Builder::new_multi_thread();
            runtime.worker_threads(dbg!(num_threads - 1));
        }
    };

    runtime.enable_all();
    let runtime = runtime.build().unwrap();

    let tasks = match args.test_tasks.and_then(NonZeroUsize::new) {
        Some(tasks) => tasks,
        None => threads,
    };

    #[derive(Debug)]
    enum TestState {
        Start {
            info: TestInfo,
        },
        Done {
            start: SystemTime,
            outcome: Outcome,
            info: TestInfo,
            slow: bool,
        },
        Tick {
            elapsed: Duration,
            info: TestInfo,
        },
    }

    let mut stats = RunStats {
        initial_run_count: tests.tasks.len(),
        finished_count: 0,
        passed: 0,
        passed_slow: 0,
        failed: 0,
        failed_slow: 0,
        timed_out: 0,
        skipped: 0,
    };

    // let mut active = HashSet::new();
    let semaphore = Arc::new(Semaphore::new(tasks.get()));
    let (tx, mut rx) = tokio::sync::mpsc::channel(tasks.get() * 4);

    for test in tests.tasks.drain(..) {
        if args.is_ignored(&test) {
            stats.skipped += 1;
        } else {
            let tx = tx.clone();
            let semaphore = semaphore.clone();
            let wait = Duration::from_secs(60);
            let test_task = async move {
                let _permit = semaphore.acquire_owned().await.unwrap();
                let start = SystemTime::now();

                let mut test_task = std::pin::pin!(CatchUnwind((test.runner)().into()));

                tx.send(TestState::Start {
                    info: test.info.clone(),
                })
                .await
                .unwrap();
                for i in 1.. {
                    let res = tokio::time::timeout(wait, test_task.as_mut()).await;
                    match res {
                        Err(_) => {
                            tx.send(TestState::Tick {
                                elapsed: i * wait,
                                info: test.info.clone(),
                            })
                            .await
                            .unwrap();
                        }
                        Ok(outcome) => {
                            tx.send(TestState::Done {
                                start,
                                outcome,
                                info: test.info,
                                slow: i > 1,
                            })
                            .await
                            .unwrap();

                            break;
                        }
                    }
                }
            };
            runtime.spawn(test_task);
        }
    }

    drop(tx);

    reporter
        .report_event(TestEvent::RunStarted {
            test_list: &test_list,
            run_id,
        })
        .unwrap();

    let mut running = 0;
    runtime.block_on(async {
        loop {
            // don't log
            let msg = rx.recv().await;

            match msg {
                Some(TestState::Start { info }) => {
                    running += 1;
                    reporter
                        .report_event(TestEvent::TestStarted {
                            test_instance: TestInstance { name: info.name },
                            current_stats: stats,
                            running,
                            cancel_state: None,
                        })
                        .unwrap()
                }
                Some(TestState::Tick { elapsed, info }) => reporter
                    .report_event(TestEvent::TestSlow {
                        test_instance: TestInstance { name: info.name },
                        elapsed,
                        will_terminate: false,
                    })
                    .unwrap(),
                Some(TestState::Done {
                    start,
                    outcome,
                    info,
                    slow,
                }) => {
                    running -= 1;
                    let status = match outcome {
                        Outcome::Passed => {
                            stats.passed += 1;
                            stats.finished_count += 1;
                            if slow {
                                stats.passed_slow += 1;
                            }
                            ExecuteStatus {
                                output: None,
                                result: nextest::ExecutionResult::Pass,
                                start_time: start,
                                time_taken: start.elapsed().unwrap(),
                                is_slow: false,
                                delay_before_start: Duration::ZERO,
                            }
                        }
                        Outcome::Failed(failed) => {
                            stats.failed += 1;
                            stats.finished_count += 1;
                            if slow {
                                stats.failed_slow += 1;
                            }
                            ExecuteStatus {
                                output: Some(failed),
                                result: nextest::ExecutionResult::Fail,
                                start_time: start,
                                time_taken: start.elapsed().unwrap(),
                                is_slow: false,
                                delay_before_start: Duration::ZERO,
                            }
                        }
                        Outcome::Ignored => return,
                    };
                    reporter
                        .report_event(TestEvent::TestFinished {
                            test_instance: TestInstance { name: info.name },
                            success_output: nextest::reporter::TestOutputDisplay::Never,
                            failure_output: nextest::reporter::TestOutputDisplay::Immediate,
                            junit_store_success_output: false,
                            junit_store_failure_output: false,
                            run_status: status,
                            current_stats: stats,
                            running,
                            cancel_state: None,
                        })
                        .unwrap();
                }
                None => break,
            }
        }
    });

    reporter
        .report_event(TestEvent::RunFinished {
            run_id,
            start_time: start_instant,
            elapsed: start_instant.elapsed().unwrap(),
            run_stats: stats,
        })
        .unwrap();

    // // Print failures if there were any, and the final summary.
    // if !failed_tests.is_empty() {
    //     printer.print_failures(&failed_tests);
    // }

    // printer.print_summary(&conclusion, start_instant.elapsed());

    conclusion
}

/// Runs the given runner, catching any panics and treating them as a failed test.
async fn run_single(
    runner: Box<dyn FnOnce() -> Box<dyn Future<Output = ()> + Send> + Send>,
    info: TestInfo,
) -> (Outcome, TestInfo, SystemTime) {
    let start = SystemTime::now();

    let res = CatchUnwind(runner().into()).await;

    (res, info, start)
}

struct CatchUnwind(Pin<Box<dyn Future<Output = ()> + Send>>);
impl Future for CatchUnwind {
    type Output = Outcome;

    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        use std::panic::{catch_unwind, AssertUnwindSafe};

        // don't log panics, catch and record them instead
        std::panic::set_hook(Box::new(|_info| {}));
        let res = catch_unwind(AssertUnwindSafe(|| self.0.as_mut().poll(cx)));
        let _ = std::panic::take_hook();

        res.map_or_else(
            |e| {
                // The `panic` information is just an `Any` object representing the
                // value the panic was invoked with. For most panics (which use
                // `panic!` like `println!`), this is either `&str` or `String`.
                let payload = e
                    .downcast_ref::<String>()
                    .map(|s| s.as_str())
                    .or(e.downcast_ref::<&str>().copied());

                let msg = payload.unwrap_or("test panicked");
                Poll::Ready(Outcome::Failed(msg.to_owned()))
            },
            |res| match res {
                Poll::Ready(()) => Poll::Ready(Outcome::Passed),
                Poll::Pending => Poll::Pending,
            },
        )
    }
}

#[macro_export]
macro_rules! test {
    ($vis:vis async fn $name:ident() $body:block) => {
        $vis async fn $name() {
            {
                $crate::__sus::inventory::submit! { $crate::__sus::TestBuilder(test_builder) }
                fn test_builder(tester: $crate::Tester) {
                    tester.add($crate::Trial::test(stringify!($name), || async { $body }));
                }
            }
            {
                $body
            }
        }
    };
}

#[macro_export]
macro_rules! tests {
    ($(#[$meta:meta])* $vis:vis fn $name:ident($tester:ident: $tester_ty:ty) $body:block) => {
        $(#[$meta])* $vis fn $name($tester: $tester_ty) {
            {
                $crate::__sus::inventory::submit! { $crate::__sus::TestBuilder($name) }
            }
            {
                $body
            }
        }
    };
}

#[doc(hidden)]
pub mod __sus {
    pub use crate::builder::TestBuilder;
    pub use inventory;
}
