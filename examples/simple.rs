extern crate libtest_mimic;

use libtest_mimic::{Arguments, TestBuilder, Tester, Trial};
use std::time;

inventory::submit! {TestBuilder(tests)}
fn tests(tester: Tester) {
    tester.add(Trial::test("check_toph", check_toph));
    tester.add(Trial::test("check_sokka", check_sokka));
    tester.add(Trial::test("long_computation", long_computation).with_ignored_flag(true));
    tester.add(Trial::test("foo", compile_fail_dummy).with_kind("compile-fail"));
    tester.add(Trial::test("check_katara", check_katara));
}

fn main() {
    let args = Arguments::from_args();
    libtest_mimic::run(&args).exit();
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
async fn compile_fail_dummy() {}
