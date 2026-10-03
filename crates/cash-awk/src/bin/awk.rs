//! The bundled awk as a program of its own, for testing it outside the shell.

fn main() {
    let code = cash_awk::run_awk(std::env::args_os());
    std::process::exit(code);
}
