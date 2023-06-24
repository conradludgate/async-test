extern crate async_test;

use async_test::{Tester, Trial};
use std::time;

fn main() {
    async_test::main()
}

// Tests

async_test::test!(
    async fn check_toph() {}
);

async_test::test!(
    async fn check_katara() {}
);

async_test::test!(
    async fn check_sokka() {
        panic!("Sokka tripped and fell :(")
    }
);

async_test::tests!(
    fn other_tests(tester: Tester) {
        for i in 0..20 {
            tester.add(Trial::test(
                format!("short_computation_{i}"),
                short_computation,
            ));
        }

        tester.add(Trial::test("long_computation", long_computation).with_ignored_flag(true));
        tester.add(Trial::test("foo", compile_fail_dummy).with_kind("compile-fail"));
    }
);

async fn long_computation() {
    tokio::time::sleep(time::Duration::from_secs(1)).await;
}
async fn short_computation() {
    tokio::time::sleep(time::Duration::from_millis(50)).await;
}

async fn compile_fail_dummy() {}
