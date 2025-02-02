use async_test::{Arguments, Tester, Trial};

async_test::tests!(
    fn tests(tester: Tester) {
        let outer_thread = std::thread::current().id();

        tester.add(Trial::test("check", move || async move {
            assert_eq!(outer_thread, std::thread::current().id());
        }));
    }
);

#[test]
fn check_test_on_main_thread() {
    let args = Arguments {
        test_threads: Some(1),
        ..Arguments::default()
    };

    let conclusion = async_test::run(&args);

    assert_eq!(conclusion.num_passed, 1);
}
