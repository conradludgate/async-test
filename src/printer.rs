//! Definition of the `Printer`.
//!
//! This is just an abstraction for everything that is printed to the screen
//! (or logfile, if specified). These parameters influence printing:
//! - `color`
//! - `format` (and `quiet`)
//! - `logfile`

use std::{
    fs::File,
    io::{self, stdout},
};

use crate::{Arguments, Trial};

pub(crate) struct Printer {
    out: Box<dyn io::Write>,
}

impl Printer {
    /// Creates a new printer configured by the given arguments (`format`,
    /// `quiet`, `color` and `logfile` options).
    pub(crate) fn new(args: &Arguments) -> Self {
        // Determine target of all output
        let out = if let Some(logfile) = &args.logfile {
            let f = File::create(logfile).expect("failed to create logfile");
            Box::new(f) as Box<_>
        } else {
            Box::new(stdout()) as Box<_>
        };

        Self { out }
    }

    /// Prints a list of all tests. Used if `--list` is set.
    pub(crate) fn print_list(&mut self, tests: &[Trial], ignored: bool) {
        Self::write_list(tests, ignored, &mut self.out).unwrap();
    }

    pub(crate) fn write_list(
        tests: &[Trial],
        ignored: bool,
        mut out: impl std::io::Write,
    ) -> std::io::Result<()> {
        for test in tests {
            // libtest prints out:
            // * all tests without `--ignored`
            // * just the ignored tests with `--ignored`
            if ignored && !test.info.is_ignored {
                continue;
            }

            writeln!(out, "{}: test", test.info.name)?;
        }

        Ok(())
    }
}
