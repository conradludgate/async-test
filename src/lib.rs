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
#![allow(clippy::all, unused_variables, dead_code)]

mod nextest;

use std::{
    any::TypeId,
    backtrace::{Backtrace, BacktraceStatus},
    cell::Cell,
    collections::HashMap,
    future::Future,
    num::NonZeroUsize,
    pin::Pin,
    process,
    sync::{Arc, Mutex},
    task::Poll,
    time::{Duration, SystemTime},
};

mod args;
mod printer;

use nextest::{
    reporter::{ReporterOutput, TestEvent, TestReporterBuilder},
    ExecuteStatus, MismatchReason, RunStats, TestInstance, TestList,
};
use tokio::sync::Semaphore;

pub use crate::args::{Arguments, ColorSetting, FormatSetting};

type Fut = Pin<Box<dyn 'static + Send + Future<Output = ()>>>;
type Fun = Box<dyn 'static + Send + FnOnce(&'static Context) -> Fut>;
/// A single test.
///
/// The original `libtest` often calls benchmarks "tests", which is a bit
/// confusing. So in this library, it is called "trial".
///
/// A trial is created via [`Trial::test`]. The trial's
/// `name` is printed and used for filtering. The `runner` is called when the
/// test/benchmark is executed to determine its outcome. If `runner` panics,
/// the trial is considered "failed". If you need the behavior of
/// `#[should_panic]` you need to catch the panic yourself. You likely want to
/// compare the panic payload to an expected value anyway.
pub struct Trial {
    runner: Option<Fun>,
    requires: Vec<(&'static str, TypeId)>,
    info: TestInfo,
}

pub trait TestFn<T>: Clone + Send + Sized + 'static {
    fn call(self, context: &'static Context) -> Fut;
    fn requires(&self) -> Vec<(&'static str, TypeId)>;
}

impl<F, Fut2> TestFn<((),)> for F
where
    F: FnOnce() -> Fut2 + Clone + Send + 'static,
    Fut2: Future<Output = ()> + Send + 'static,
{
    fn call(self, context: &'static Context) -> Fut {
        Box::pin(async move {
            self().await;
        })
    }
    fn requires(&self) -> Vec<(&'static str, TypeId)> {
        vec![]
    }
}

macro_rules! impl_handler {
    (
        [$($ty:ident),*]
    ) => {
        #[allow(non_snake_case, unused_mut)]
        impl<F, Fut2, $($ty,)*> TestFn<($($ty,)* ())> for F
        where
            F: FnOnce($(&'static $ty),*) -> Fut2 + Clone + Send + 'static,
            Fut2: Future<Output = ()> + Send + 'static,
            $($ty: 'static + Sync + Send,)*
        {
            fn call(self, context: &'static Context) -> Fut {
                Box::pin(async move {
                    $(
                        let $ty: &'static $ty = context.get().await.unwrap();
                    )*

                    self($($ty),*).await;
                })
            }
            fn requires(&self) -> Vec<(&'static str, TypeId)> {
                vec![$((std::any::type_name::<$ty>(), TypeId::of::<$ty>())),*]
            }
        }
    };
}

impl_handler!([T1]);
impl_handler!([T1, T2]);
impl_handler!([T1, T2, T3]);
impl_handler!([T1, T2, T3, T4]);
impl_handler!([T1, T2, T3, T4, T5]);
impl_handler!([T1, T2, T3, T4, T5, T6]);
impl_handler!([T1, T2, T3, T4, T5, T6, T7]);
impl_handler!([T1, T2, T3, T4, T5, T6, T7, T8]);
impl_handler!([T1, T2, T3, T4, T5, T6, T7, T8, T9]);
impl_handler!([T1, T2, T3, T4, T5, T6, T7, T8, T9, T10]);
impl_handler!([T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11]);
impl_handler!([T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12]);
impl_handler!([T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13]);
impl_handler!([T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14]);
impl_handler!([T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15]);

impl Trial {
    /// Creates a (non-benchmark) test with the given name and runner.
    ///
    /// The runner returning `Ok(())` is interpreted as the test passing. If the
    /// runner returns `Err(_)`, the test is considered failed.
    pub fn test<T, F>(name: impl Into<String>, runner: F) -> Self
    where
        T: 'static,
        F: TestFn<T>,
    {
        Self {
            requires: runner.requires(),
            runner: Some(Box::new(move |ctx| Box::pin(runner.call(ctx)))),
            info: TestInfo {
                name: name.into(),
                is_ignored: false,
            },
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

    /// Returns whether this trial has been marked as *ignored*.
    pub fn has_ignored_flag(&self) -> bool {
        self.info.is_ignored
    }
}

// struct Config {}

// type AnyOwnedVal = Box<dyn std::any::Any + Send + Sync + 'static>;
type AnySharedVal = Arc<dyn std::any::Any + Send + Sync>;

struct Setup {
    // type_id: fn() -> &'static TypeId,
    module: &'static str,
    function: &'static str,
    // file: &'static str,
    // line: u32,
    setup: fn() -> tokio::task::JoinHandle<AnySharedVal>,
    // init: AtomicUsize,
    value: tokio::sync::OnceCell<AnySharedVal>,
}

impl Context {
    async fn get<T: 'static>(&'static self) -> Option<&'static T> {
        let id = TypeId::of::<T>();
        match self.values.get(&id) {
            Some(s) => Some(s.get().await),
            None => None,
        }
    }
}

impl Setup {
    async fn get<T: 'static>(&'static self) -> &'static T {
        // &** is necessary... trust me
        // get_or_init returns &Arc<T>
        // first  * removes outer ref -> Arc<T>
        // second * removes Arc       -> T
        // final  & makes a ref again -> &T
        let x: &'static dyn std::any::Any = &**self.value.get().expect("setup should be init");
        x.downcast_ref().expect("type should be correct")
    }
    // async fn load(&'static self) -> &AnySharedVal {
    //     self.init.fetch_add(1, Ordering::AcqRel);
    //     self.value
    //         .get_or_init(|| async { (self.setup)().await.unwrap() })
    //         .await
    // }
}

pub struct Context {
    values: HashMap<TypeId, Arc<Setup>>,
}

#[derive(Clone)]
pub struct Tester {
    context: &'static Context,
    inner: Arc<Mutex<TesterInner>>,
}

impl Tester {
    pub fn add(&self, trial: Trial) {
        let mut missing = vec![];
        for (ty, id) in &trial.requires {
            if !self.context.values.contains_key(id) {
                missing.push(ty);
            }
        }

        if !missing.is_empty() {
            let mut types = String::new();
            for missing in missing {
                types += "\n\t";
                types += missing;
            }

            panic!(
                "Test '{}' is missing required setup methods for:{}",
                trial.info.name, types
            );
        }

        self.inner.lock().unwrap().tasks.push(trial)
    }
}

struct TesterInner {
    tasks: Vec<Trial>,
}

mod builder {
    use std::{any::TypeId, marker::PhantomData};

    use crate::{AnySharedVal, Tester};

    pub trait TestRequirementHasSetupFnFor<T> {}

    pub struct Setup<T>(PhantomData<T>);

    pub struct SetupInit {
        pub type_id: fn() -> TypeId,
        pub module: &'static str,
        pub function: &'static str,
        // file: &'static str,
        // line: u32,
        pub setup: fn() -> tokio::task::JoinHandle<AnySharedVal>,
    }
    inventory::collect!(SetupInit);

    pub struct TestBuilder(pub fn(tester: Tester));
    inventory::collect!(TestBuilder);
}

fn setup_tests() -> (Vec<Trial>, &'static Context) {
    let mut context = Context {
        values: HashMap::new(),
    };
    for setup in inventory::iter::<builder::SetupInit>() {
        context.values.insert(
            (setup.type_id)(),
            Arc::new(Setup {
                module: setup.module,
                function: setup.function,
                // file: setup.file,
                // line: setup.line,
                setup: setup.setup,
                value: tokio::sync::OnceCell::new(),
            }),
        );
    }
    let context: &'static Context = Box::leak(Box::new(context));
    let tester = Tester {
        context,
        inner: Arc::new(Mutex::new(TesterInner { tasks: vec![] })),
    };
    for builder in inventory::iter::<builder::TestBuilder>() {
        (builder.0)(tester.clone())
    }
    let tasks = std::mem::take(&mut tester.inner.lock().unwrap().tasks);
    (tasks, context)
}

#[derive(Debug, Clone)]
pub(crate) struct TestInfo {
    name: String,
    is_ignored: bool,
}

/// The outcome of performing a test/benchmark.
#[derive(Debug, Clone)]
enum Outcome {
    /// The test passed.
    Passed,

    /// The test failed.
    Failed(String),
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
    pub num_filtered_out: usize,

    /// Number of passed tests.
    pub num_passed: usize,

    /// Number of failed tests and benchmarks.
    pub num_failed: usize,
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
        }
    }
}

impl Arguments {
    /// Returns `true` if the given test should be ignored.
    fn is_ignored(&self, test: &Trial) -> bool {
        test.info.is_ignored && !self.ignored && !self.include_ignored
    }

    fn is_filtered_out(&self, test: &Trial) -> Option<MismatchReason> {
        let test_name = &test.info.name;

        // If a filter was specified, apply this
        let matches_filter = self.filter.iter().any(|filter| match self.exact {
            true if test_name == filter => true,
            false if test_name.contains(filter) => true,
            _ => false,
        });
        if !self.filter.is_empty() && !matches_filter {
            return Some(MismatchReason::String);
        }

        // If any skip pattern were specified, test for all patterns.
        let matches_skip = self.skip.iter().any(|skip_filter| match self.exact {
            true if test_name == skip_filter => true,
            false if test_name.contains(skip_filter) => true,
            _ => false,
        });
        if matches_skip {
            return Some(MismatchReason::String);
        }

        if self.ignored ^ test.info.is_ignored {
            return Some(MismatchReason::Ignored);
        }

        None
    }
}

/// Runs all given tests.
///
/// This is the central function of this crate. It provides the framework for
/// the testing harness. It does all the printing and house keeping.
pub fn main() {
    let args = Arguments::from_args();
    let c = run(&args);
    c.exit_if_failed();
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

    let (mut tests, context) = setup_tests();

    // If `--list` is specified, just print the list and return.
    if args.list {
        if !args.filter.is_empty() || !args.skip.is_empty() || args.ignored {
            tests.retain(|test| args.is_filtered_out(test).is_none());
        }

        let mut printer = printer::Printer::new(args);
        printer.print_list(&tests, args.ignored);
        return Conclusion::empty();
    }

    run_nextest(args, start_instant, &mut tests, context)
}

struct Location {
    file: String,
    line: u32,
    column: u32,
}

thread_local! {
    static BT: Cell<(Backtrace, Option<Location>)> = Cell::new((Backtrace::disabled(), None));
}

fn run_nextest(
    args: &Arguments,
    start_instant: SystemTime,
    tests: &mut [Trial],
    context: &'static Context,
) -> Conclusion {
    let mut test_list = TestList {
        tests: vec![],
        skip_count: 0,
    };

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
            runtime.worker_threads(num_threads - 1);
        }
    };

    let runtime = runtime.enable_all().build().unwrap();

    let tasks = match args.test_tasks.and_then(NonZeroUsize::new) {
        Some(tasks) => tasks,
        None => threads,
    };

    #[derive(Debug)]
    enum TestState {
        Skipped {
            name: String,
            reason: MismatchReason,
        },
        Start {},
        StartSetup {},
        DoneSetup {
            name: String,
            start: SystemTime,
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

    let slow_period = Duration::from_secs(15);

    let semaphore = Arc::new(Semaphore::new(tasks.get()));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let mut stats = RunStats::default();

    // don't log panics, catch and record them instead
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let bt = std::backtrace::Backtrace::capture();
        let location = info.location().map(|loc| Location {
            file: loc.file().to_owned(),
            line: loc.line(),
            column: loc.column(),
        });
        BT.with(|x| x.set((bt, location)));
    }));

    for test in tests.iter_mut() {
        if let Some(reason) = args.is_filtered_out(&test) {
            stats.skipped += 1;
            test_list.skip_count += 1;
        } else {
            stats.initial_run_count += 1;
            test_list.tests.push(test.info.clone());

            let req_len = test.requires.len() as u32;
            let wg = Arc::new(Semaphore::new(req_len as usize));

            for (requirement, id) in &test.requires {
                if let Some(s) = context.values.get(&id) {
                    let tx = tx.clone();
                    let permit = semaphore.clone().acquire_owned();
                    let wg_permit = wg.clone().try_acquire_owned().unwrap();
                    runtime.spawn(async move {
                        let _wg_permit = wg_permit;
                        s.value
                            .get_or_init(move || async move {
                                let _permit = permit.await.unwrap();
                                let start = SystemTime::now();

                                tx.send(TestState::StartSetup {}).unwrap();
                                let res = (s.setup)().await.unwrap();
                                tx.send(TestState::DoneSetup {
                                    name: s.function.to_owned(),
                                    start,
                                })
                                .unwrap();
                                res
                            })
                            .await;
                    });
                }
            }

            let tx = tx.clone();
            let permit = semaphore.clone().acquire_owned();
            let runner = test.runner.take().unwrap();
            let task = runner(context);
            let info = test.info.clone();
            let test_task = async move {
                let _wg_permit = wg.acquire_many_owned(req_len).await.unwrap();
                let _permit = permit.await.unwrap();
                let start = SystemTime::now();

                let mut test_task = std::pin::pin!(CatchUnwind(task));

                tx.send(TestState::Start {}).unwrap();
                for i in 1.. {
                    let res = tokio::time::timeout(slow_period, test_task.as_mut()).await;
                    match res {
                        Err(_) => {
                            tx.send(TestState::Tick {
                                elapsed: i * slow_period,
                                info: info.clone(),
                            })
                            .unwrap();
                        }
                        Ok(outcome) => {
                            tx.send(TestState::Done {
                                start,
                                outcome,
                                info,
                                slow: i > 1,
                            })
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

    let mut output = args
        .logfile
        .as_deref()
        .map(|f| std::fs::File::create(f).unwrap());
    let report_output = match &mut output {
        Some(file) => ReporterOutput::Buffer(file),
        None => ReporterOutput::Stderr,
    };

    let mut reporter = TestReporterBuilder::default()
        .set_imitate_cargo(args.exact)
        .build(&test_list, report_output);

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

    reporter
        .report_event(TestEvent::RunStarted {
            test_list: &test_list,
        })
        .unwrap();

    let mut running = 0;
    runtime.block_on(async {
        loop {
            let msg = rx.recv().await;

            match msg {
                Some(TestState::Skipped { name, reason }) => {
                    reporter
                        .report_event(TestEvent::TestSkipped {
                            test_instance: TestInstance { name },
                            reason,
                        })
                        .unwrap();
                }
                Some(TestState::StartSetup {}) => {}
                Some(TestState::DoneSetup { name, start }) => {
                    reporter
                        .report_event(TestEvent::SetupFinished {
                            test_instance: TestInstance { name },
                            duration: start.elapsed().unwrap(),
                            current_stats: stats,
                            running,
                        })
                        .unwrap();
                }
                Some(TestState::Start {}) => {
                    running += 1;
                    reporter
                        .report_event(TestEvent::TestStarted {
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
                            stats.passed_slow += slow as usize;
                            stats.finished_count += 1;
                            ExecuteStatus {
                                output: None,
                                result: nextest::ExecutionResult::Pass,
                                start_time: start,
                                time_taken: start.elapsed().unwrap(),
                                is_slow: slow,
                                delay_before_start: Duration::ZERO,
                            }
                        }
                        Outcome::Failed(failed) => {
                            stats.failed += 1;
                            stats.failed_slow += slow as usize;
                            stats.finished_count += 1;
                            ExecuteStatus {
                                output: Some(failed),
                                result: nextest::ExecutionResult::Fail,
                                start_time: start,
                                time_taken: start.elapsed().unwrap(),
                                is_slow: slow,
                                delay_before_start: Duration::ZERO,
                            }
                        }
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

    std::panic::set_hook(hook);

    reporter
        .report_event(TestEvent::RunFinished {
            start_time: start_instant,
            elapsed: start_instant.elapsed().unwrap(),
            run_stats: stats,
        })
        .unwrap();

    Conclusion {
        num_filtered_out: stats.skipped,
        num_passed: stats.passed,
        num_failed: stats.failed,
    }
}

struct CatchUnwind(Pin<Box<dyn Future<Output = ()> + Send>>);
impl Future for CatchUnwind {
    type Output = Outcome;

    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        use std::panic::{catch_unwind, AssertUnwindSafe};

        let res = catch_unwind(AssertUnwindSafe(|| self.0.as_mut().poll(cx)));

        match res {
            Err(e) => {
                // The `panic` information is just an `Any` object representing the
                // value the panic was invoked with. For most panics (which use
                // `panic!` like `println!`), this is either `&str` or `String`.
                let payload = e
                    .downcast_ref::<String>()
                    .map(|s| s.as_str())
                    .or(e.downcast_ref::<&str>().copied());

                let msg = payload.unwrap_or("test panicked");

                let (bt, location) = BT.with(|x| x.replace((Backtrace::disabled(), None)));
                // dbg!(location);

                let mut final_msg = format!("thread 'main' panicked at '{msg}'");
                if let Some(Location { file, line, column }) = location {
                    final_msg += &format!(", {file}:{line}:{column}");
                }
                if bt.status() == BacktraceStatus::Captured {
                    let bt = bt.to_string();
                    if let Some(unwind) = bt.find("rust_begin_unwind") {
                        if let Some(catch) = bt[unwind..].find("async_test::CatchUnwind") {
                            let unwind_start = bt[..unwind].rfind('\n').unwrap_or(0);
                            let catch_start = bt[..unwind + catch].rfind('\n').unwrap();
                            final_msg += &format!(
                                "\nstack backtrace:\n{}",
                                bt[unwind_start..catch_start].trim_start_matches('\n')
                            );
                        }
                    }
                }

                Poll::Ready(Outcome::Failed(final_msg))
            }
            Ok(Poll::Ready(())) => Poll::Ready(Outcome::Passed),
            Ok(Poll::Pending) => Poll::Pending,
        }
    }
}

#[macro_export]
macro_rules! test {
    ($vis:vis async fn $name:ident($($arg:ident: $arg_ty:ty),* $(,)?) $body:block) => {
        $vis async fn $name($($arg: $arg_ty),*) {
            {
                // $($crate::__sus::has_setup_fn::<_, $arg_ty>();)*
                $crate::__sus::inventory::submit! {
                    $crate::__sus::TestBuilder(
                        |tester: $crate::Tester| tester.add($crate::Trial::test(stringify!($name), $name))
                    )
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

#[macro_export]
macro_rules! setup {
    ($(#[$meta:meta])* $vis:vis async fn $name:ident() -> $setup:ty $body:block) => {
        #[doc(hidden)]
        #[allow(non_camel_case_types)]
        $vis struct $name {}
        #[doc(hidden)]
        const _: () = {
            use $crate::__sus::{TestRequirementHasSetupFnFor, Setup};

            impl TestRequirementHasSetupFnFor<&$setup> for Setup<$name> {}
        };
        $(#[$meta])* $vis async fn $name() -> $setup {
            {
                $crate::__sus::inventory::submit! {
                    $crate::__sus::SetupInit{
                        type_id: $crate::__sus::TypeId::of::<$setup>,
                        module: $crate::__sus::module_path!(),
                        function: stringify!($name),
                        setup: || $crate::__sus::spawn(async {
                            let x: $setup = $name().await;
                            $crate::__sus::Arc::new(x) as $crate::__sus::Arc<_>
                        }),
                    }
                }
            }
            {
                $body
            }
        }
    };
}

#[doc(hidden)]
pub mod __sus {
    pub use crate::builder::SetupInit;
    pub use crate::builder::TestBuilder;
    pub use crate::builder::{Setup, TestRequirementHasSetupFnFor};
    pub use inventory;
    pub use std::sync::Arc;
    pub use std::{any::TypeId, module_path};
    pub use tokio::task::spawn;

    pub fn has_setup_fn<T, S>()
    where
        Setup<T>: TestRequirementHasSetupFnFor<S>,
    {
    }
}
