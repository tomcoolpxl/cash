//! `bc` — ROADMAP item 10, part 3: POSIX `bc`, absorbed from posixutils-rs.
//!
//! The upstream test suite (`crates/cash-bc/tests/cases/mod.rs` and its `.bc`/`.out`
//! pairs) runs here unchanged apart from where it finds its helpers: the `plib` module
//! below stands in for posixutils' test library, running `bc` as cash runs it — the
//! bundled command behind `cash -c 'bc …'`. Cash's own checks are in `bc_cash.rs`.

#![allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::restriction,
    reason = "the upstream suite is imported as written; its style is posixutils', not \
              this workspace's"
)]

#[path = "../../../cash-bc/tests/cases/mod.rs"]
mod upstream;

/// The parts of posixutils' `plib` test library the upstream suite uses. The suite
/// reaches it as `crate::plib`, so main.rs brings it to the crate root.
pub(crate) mod plib {
    pub mod testing {
        use std::io::Write as _;
        use std::process::{Command, Output, Stdio};

        const CASH: &str = env!("CARGO_BIN_EXE_cash");

        /// One run of a utility and what it must produce.
        pub struct TestPlan {
            pub cmd: String,
            pub args: Vec<String>,
            pub stdin_data: String,
            pub expected_out: String,
            pub expected_err: String,
            pub expected_exit_code: i32,
        }

        /// `cash -c 'CMD ARGS…'` with `stdin` on standard input.
        pub fn run_test_base(cmd: &str, args: &[String], stdin: &[u8]) -> Output {
            let quoted: Vec<String> = args
                .iter()
                .map(|a| format!("'{}'", a.replace('\'', r"'\''")))
                .collect();
            let script = format!("{cmd} {}", quoted.join(" "));
            let mut child = Command::new(CASH)
                .args(["-c", &script])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("run cash");
            let mut input = child.stdin.take().expect("stdin");
            input.write_all(stdin).expect("write stdin");
            drop(input);
            child.wait_with_output().expect("wait for cash")
        }

        pub fn run_test(plan: TestPlan) {
            let output = run_test_base(&plan.cmd, &plan.args, plan.stdin_data.as_bytes());
            assert_eq!(String::from_utf8_lossy(&output.stdout), plan.expected_out);
            assert_eq!(String::from_utf8_lossy(&output.stderr), plan.expected_err);
            assert_eq!(output.status.code(), Some(plan.expected_exit_code));
        }

        /// Only the `/dev/full` test asks for this, and it returns before using it on a
        /// host without `/dev/full`, which Windows is.
        pub fn get_binary_path(_name: &str) -> std::path::PathBuf {
            std::path::PathBuf::from(CASH)
        }
    }

    pub mod tmp {
        pub use tempfile::Builder;
    }
}
