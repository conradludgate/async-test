use crate::common::{args, check, do_run};
use async_test::{Conclusion, Tester, Trial};
use pretty_assertions::assert_eq;

#[macro_use]
mod common;

// async_test::test!(
//     async fn cat() {}
// );

// async_test::test!(
//     async fn dog() {}
// );

// async_test::test!(
//     async fn fox() {}
// );

// async_test::test!(
//     async fn bunny() {}
// );

// async_test::test!(
//     async fn frog() {}
// );

// async_test::test!(
//     async fn owl() {}
// );

// async_test::test!(
//     async fn fly() {}
// );

// async_test::test!(
//     async fn bear() {}
// );

async_test::tests!(
    fn tests(tester: Tester) {
        tester.add(Trial::test("cat", || async {}));
        tester.add(Trial::test("dog", || async {
            panic!("was not a good boy")
        }));
        tester.add(Trial::test("fox", || async {}).with_kind("apple"));
        tester.add(Trial::test("bunny", || async { panic!("jumped too high") }).with_kind("apple"));
        tester.add(Trial::test("frog", || async {}).with_ignored_flag(true));
        tester.add(Trial::test("owl", || async { panic!("broke neck") }).with_ignored_flag(true));
        tester.add(
            Trial::test("fly", || async {})
                .with_ignored_flag(true)
                .with_kind("banana"),
        );
        tester.add(
            Trial::test("bear", || async { panic!("no honey") })
                .with_ignored_flag(true)
                .with_kind("banana"),
        );
    }
);

#[test]
fn normal() {
    check(
        args([]),
        8,
        Conclusion {
            num_filtered_out: 0,
            num_passed: 2,
            num_failed: 2,
            num_ignored: 4,
        },
        "
            test          cat   ... ok
            test          dog   ... FAILED
            test [apple]  fox   ... ok
            test [apple]  bunny ... FAILED
            test          frog  ... ignored
            test          owl   ... ignored
            test [banana] fly   ... ignored
            test [banana] bear  ... ignored

            failures:

            ---- dog ----
            was not a good boy

            ---- bunny ----
            jumped too high


            failures:
                dog
                bunny
        ",
    );
}

#[test]
fn test_mode() {
    check(
        args(["--test"]),
        8,
        Conclusion {
            num_filtered_out: 0,
            num_passed: 2,
            num_failed: 2,
            num_ignored: 4,
        },
        "
            test          cat   ... ok
            test          dog   ... FAILED
            test [apple]  fox   ... ok
            test [apple]  bunny ... FAILED
            test          frog  ... ignored
            test          owl   ... ignored
            test [banana] fly   ... ignored
            test [banana] bear  ... ignored

            failures:

            ---- dog ----
            was not a good boy

            ---- bunny ----
            jumped too high


            failures:
                dog
                bunny
        ",
    );
}

#[test]
fn list() {
    let (c, out) = common::do_run(args(["--list"]));
    assert_log!(
        out,
        "
        cat: test
        dog: test
        [apple] fox: test
        [apple] bunny: test
        frog: test
        owl: test
        [banana] fly: test
        [banana] bear: test
    "
    );
    assert_eq!(
        c,
        Conclusion {
            num_filtered_out: 0,
            num_passed: 0,
            num_failed: 0,
            num_ignored: 0,
        }
    );
}

#[test]
fn list_ignored() {
    let (c, out) = common::do_run(args(["--list", "--ignored"]));
    assert_log!(
        out,
        "
        frog: test
        owl: test
        [banana] fly: test
        [banana] bear: test
    "
    );
    assert_eq!(
        c,
        Conclusion {
            num_filtered_out: 0,
            num_passed: 0,
            num_failed: 0,
            num_ignored: 0,
        }
    );
}

#[test]
fn list_with_filter() {
    let (c, out) = common::do_run(args(["--list", "a"]));
    assert_log!(
        out,
        "
        cat: test
        [banana] bear: test
    "
    );
    assert_eq!(
        c,
        Conclusion {
            num_filtered_out: 0,
            num_passed: 0,
            num_failed: 0,
            num_ignored: 0,
        }
    );
}

#[test]
fn filter_c() {
    check(
        args(["c"]),
        1,
        Conclusion {
            num_filtered_out: 7,
            num_passed: 1,
            num_failed: 0,
            num_ignored: 0,
        },
        "
            test cat ... ok
        ",
    );
}

#[test]
fn filter_o_test() {
    check(
        args(["--test", "o"]),
        4,
        Conclusion {
            num_filtered_out: 4,
            num_passed: 1,
            num_failed: 1,
            num_ignored: 2,
        },
        "
            test         dog  ... FAILED
            test [apple] fox  ... ok
            test         frog ... ignored
            test         owl  ... ignored

            failures:

            ---- dog ----
            was not a good boy


            failures:
                dog
        ",
    );
}

#[test]
fn filter_o_test_include_ignored() {
    check(
        args(["--test", "--include-ignored", "o"]),
        4,
        Conclusion {
            num_filtered_out: 4,
            num_passed: 2,
            num_failed: 2,
            num_ignored: 0,
        },
        "
            test         dog  ... FAILED
            test [apple] fox  ... ok
            test         frog ... ok
            test         owl  ... FAILED

            failures:

            ---- dog ----
            was not a good boy

            ---- owl ----
            broke neck


            failures:
                dog
                owl
        ",
    );
}

#[test]
fn filter_o_test_ignored() {
    check(
        args(["--test", "--ignored", "o"]),
        2,
        Conclusion {
            num_filtered_out: 6,
            num_passed: 1,
            num_failed: 1,
            num_ignored: 0,
        },
        "
            test frog ... ok
            test owl  ... FAILED

            failures:

            ---- owl ----
            broke neck


            failures:
                owl
        ",
    );
}

#[test]
fn normal_include_ignored() {
    check(
        args(["--include-ignored"]),
        8,
        Conclusion {
            num_filtered_out: 0,
            num_passed: 4,
            num_failed: 4,
            num_ignored: 0,
        },
        "
            test          cat   ... ok
            test          dog   ... FAILED
            test [apple]  fox   ... ok
            test [apple]  bunny ... FAILED
            test          frog  ... ok
            test          owl   ... FAILED
            test [banana] fly   ... ok
            test [banana] bear  ... FAILED

            failures:

            ---- dog ----
            was not a good boy

            ---- bunny ----
            jumped too high

            ---- owl ----
            broke neck

            ---- bear ----
            no honey


            failures:
                dog
                bunny
                owl
                bear
        ",
    );
}

#[test]
fn normal_ignored() {
    check(
        args(["--ignored"]),
        4,
        Conclusion {
            num_filtered_out: 4,
            num_passed: 2,
            num_failed: 2,
            num_ignored: 0,
        },
        "
            test          frog ... ok
            test          owl  ... FAILED
            test [banana] fly  ... ok
            test [banana] bear ... FAILED

            failures:

            ---- owl ----
            broke neck

            ---- bear ----
            no honey


            failures:
                owl
                bear
        ",
    );
}

#[test]
fn lots_of_flags() {
    check(
        args(["--include-ignored", "--skip", "g", "--test", "o"]),
        2,
        Conclusion {
            num_filtered_out: 6,
            num_passed: 1,
            num_failed: 1,
            num_ignored: 0,
        },
        "
            test [apple] fox ... ok
            test         owl ... FAILED

            failures:

            ---- owl ----
            broke neck


            failures:
                owl
        ",
    );
}

#[test]
fn terse_output() {
    let (c, out) = do_run(args(["--format", "terse", "--test-threads", "1"]));
    assert_eq!(
        c,
        Conclusion {
            num_filtered_out: 0,
            num_passed: 2,
            num_failed: 2,
            num_ignored: 4,
        }
    );
    assert_log!(
        out,
        "
        running 8 tests
        .F.Fiiii
        failures:

        ---- dog ----
        was not a good boy

        ---- bunny ----
        jumped too high


        failures:
            dog
            bunny

        test result: FAILED. 2 passed; 2 failed; 4 ignored; 0 filtered out; \
            finished in 0.00s
    "
    );
}
