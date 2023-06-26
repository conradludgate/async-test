extern crate async_test;

use async_test::{Tester, Trial};

use std::{env, error::Error, ffi::OsStr, fs, path::Path};

fn main() {
    async_test::main()
}

async_test::tests!(
    /// Creates one test for each `.rs` file in the current directory or
    /// sub-directories of the current directory.
    fn collect_tests(tester: Tester) {
        fn visit_dir(path: &Path, tester: &Tester) -> Result<(), Box<dyn Error>> {
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                let file_type = entry.file_type()?;

                // Handle files
                let path = entry.path();
                if file_type.is_file() {
                    if path.extension() == Some(OsStr::new("rs")) {
                        let name = path
                            .strip_prefix(env::current_dir()?)?
                            .display()
                            .to_string();

                        let test =
                            Trial::test(name, move || async move { check_file(&path).await });
                        tester.add(test);
                    }
                } else if file_type.is_dir() {
                    // Handle directories
                    visit_dir(&path, tester)?;
                }
            }

            Ok(())
        }

        // We recursively look for `.rs` files, starting from the current
        // directory.
        let current_dir = env::current_dir().unwrap();
        visit_dir(&current_dir, &tester).unwrap();
    }
);

/// Performs a couple of tidy tests.
async fn check_file(path: &Path) {
    let content = tokio::fs::read(path).await.unwrap();

    // Check that the file is valid UTF-8
    let content = String::from_utf8(content).unwrap();

    // Check for `\r`: we only want `\n` line breaks!
    if content.contains('\r') {
        panic!("Contains '\\r' chars. Please use ' \\n' line breaks only!");
    }

    // Check for tab characters `\t`
    if content.contains('\t') {
        panic!("Contains tab characters ('\\t'). Indent with four spaces!");
    }

    // Check for too long lines
    if content.lines().any(|line| line.chars().count() > 100) {
        panic!("Contains lines longer than 100 codepoints!");
    }
}
