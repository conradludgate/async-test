extern crate async_test;

use async_test::{TestBuilder, Tester, Trial};
use std::time;

inventory::submit! {TestBuilder(tests)}
fn tests(tester: Tester) {
    tester.add(Trial::test("check_toph", check_toph));
    tester.add(Trial::test("check_sokka", check_sokka));

    for i in 0..20 {
        tester.add(Trial::test(
            format!("short_computation_{i}"),
            short_computation,
        ));
    }

    tester.add(Trial::test("long_computation", long_computation).with_ignored_flag(true));
    tester.add(Trial::test("foo", compile_fail_dummy).with_kind("compile-fail"));
    tester.add(Trial::test("check_katara", check_katara));
}

fn main() {
    async_test::main()
}

// Tests

async fn check_toph() {}
async fn check_katara() {}
async fn check_sokka() {
    panic!("Sokka tripped and fell :(")
}
async fn long_computation() {
    tokio::time::sleep(time::Duration::from_secs(1)).await;
}
async fn short_computation() {
    tokio::time::sleep(time::Duration::from_millis(50)).await;
}
async fn compile_fail_dummy() {}
