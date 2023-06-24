use async_test::Conclusion;
use common::{args, check};

#[macro_use]
mod common;

async_test::test!(
    async fn panics() {
        panic!("uh oh")
    }
);

async_test::test!(
    async fn passes() {}
);

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
        },
        "
            test passes ... ok
            test panics ... FAILED

            failures:

            ---- panics ----
            uh oh


            failures:
                panics
        ",
    );
}
