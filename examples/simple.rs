extern crate async_test;

use async_test::{Tester, Trial};
use std::time;

fn main() {
    async_test::main()
}

// Tests

struct Config {}

async_test::setup!(
    async fn config() -> Config {
        Config {}
    }
);

async_test::test!(
    async fn check_foo() {}
);

async_test::test!(
    async fn check_bar(_c: &Config) {}
);

async_test::test!(
    async fn check_panic() {
        panic!("Ripperonis")
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
    }
);

async fn long_computation() {
    tokio::time::sleep(time::Duration::from_secs(20)).await;
}
async fn short_computation() {
    tokio::time::sleep(time::Duration::from_millis(200)).await;
}
