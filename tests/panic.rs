use common::{args, check};
use libtest_mimic::{Conclusion, TestBuilder, Tester, Trial};

#[macro_use]
mod common;

inventory::submit! {TestBuilder(foo)}
fn foo(tester: Tester) {
    tester.add(Trial::test("passes", || async {}));
    tester.add(Trial::test("panics", || async { panic!("uh oh") }));
}

#[test]
fn normal() {
    check(
        args([]),
        2,
        Conclusion {
            num_filtered_out: 0,
            num_passed: 1,
            num_failed: 1,
            num_ignored: 0,
            num_measured: 0,
        },
        "
            test passes ... ok
            test panics ... FAILED

            failures:

            ---- panics ----
            test panicked: uh oh


            failures:
                panics
        ",
    );
}
