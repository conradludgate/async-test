use common::{args, check};
use libtest_mimic::{Conclusion, Trial, Tester, TestBuilder};
use pretty_assertions::assert_eq;

use crate::common::do_run;

#[macro_use]
mod common;

inventory::submit! {TestBuilder(tests)}
fn tests(tester: Tester) {
    tester.add(Trial::test("foo", || async {}));
    tester.add(Trial::test("bar", || async {}));
    tester.add(Trial::test("barro", || async {}));
}

#[test]
fn normal() {
    check(
        args([]),
        3,
        Conclusion {
            num_filtered_out: 0,
            num_passed: 3,
            num_failed: 0,
            num_ignored: 0,
            num_measured: 0,
        },
        "
            test foo   ... ok
            test bar   ... ok
            test barro ... ok
        ",
    );
}

#[test]
fn filter_one() {
    check(
        args(["foo"]),
        1,
        Conclusion {
            num_filtered_out: 2,
            num_passed: 1,
            num_failed: 0,
            num_ignored: 0,
            num_measured: 0,
        },
        "test foo ... ok",
    );
}

#[test]
fn filter_two() {
    check(
        args(["bar"]),
        2,
        Conclusion {
            num_filtered_out: 1,
            num_passed: 2,
            num_failed: 0,
            num_ignored: 0,
            num_measured: 0,
        },
        "
            test bar   ... ok
            test barro ... ok
        ",
    );
}

#[test]
fn filter_exact() {
    check(
        args(["bar", "--exact"]),
        1,
        Conclusion {
            num_filtered_out: 2,
            num_passed: 1,
            num_failed: 0,
            num_ignored: 0,
            num_measured: 0,
        },
        "test bar ... ok",
    );
}

#[test]
fn filter_two_and_skip() {
    check(
        args(["--skip", "barro", "bar"]),
        1,
        Conclusion {
            num_filtered_out: 2,
            num_passed: 1,
            num_failed: 0,
            num_ignored: 0,
            num_measured: 0,
        },
        "test bar ... ok",
    );
}

#[test]
fn skip_nothing() {
    check(
        args(["--skip", "peter"]),
        3,
        Conclusion {
            num_filtered_out: 0,
            num_passed: 3,
            num_failed: 0,
            num_ignored: 0,
            num_measured: 0,
        },
        "
            test foo   ... ok
            test bar   ... ok
            test barro ... ok
        ",
    );
}

#[test]
fn skip_two() {
    check(
        args(["--skip", "bar"]),
        1,
        Conclusion {
            num_filtered_out: 2,
            num_passed: 1,
            num_failed: 0,
            num_ignored: 0,
            num_measured: 0,
        },
        "test foo ... ok",
    );
}

#[test]
fn skip_exact() {
    check(
        args(["--exact", "--skip", "bar"]),
        2,
        Conclusion {
            num_filtered_out: 1,
            num_passed: 2,
            num_failed: 0,
            num_ignored: 0,
            num_measured: 0,
        },
        "
            test foo   ... ok
            test barro ... ok
        ",
    );
}

#[test]
fn terse_output() {
    let (c, out) = do_run(args(["--format", "terse"]));
    assert_eq!(
        c,
        Conclusion {
            num_filtered_out: 0,
            num_passed: 3,
            num_failed: 0,
            num_ignored: 0,
            num_measured: 0,
        }
    );
    assert_log!(
        out,
        "
        running 3 tests
        ...
        test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; \
            finished in 0.00s
    "
    );
}
