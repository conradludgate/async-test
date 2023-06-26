use async_test::Conclusion;
use common::{args, check};
use pretty_assertions::assert_eq;

use crate::common::do_run;

#[macro_use]
mod common;

async_test::test!(
    async fn foo() {}
);

async_test::test!(
    async fn bar() {}
);

async_test::test!(
    async fn barro() {}
);

#[test]
fn normal() {
    check(
        args([]),
        3,
        Conclusion {
            num_filtered_out: 0,
            num_passed: 3,
            num_failed: 0,
        },
        "
            test barro ... ok
            test bar   ... ok
            test foo   ... ok
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
        },
        "
            test barro ... ok
            test bar   ... ok
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
        },
        "
            test barro ... ok
            test bar   ... ok
            test foo   ... ok
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
        },
        "
            test barro ... ok
            test foo   ... ok
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
        }
    );
    assert_log!(
        out,
        "
        running 3 tests
        ...
        test result: ok. 3 passed; 0 failed; 0 ignored; 0 filtered out; \
            finished in 0.00s
    "
    );
}
