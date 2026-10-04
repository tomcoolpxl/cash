//! The bundled awk as a program of its own, for testing it outside the shell.

fn main() {
    // Inside cash, awk's commands run in cash, this process's own exe; this program is
    // not cash, so they run in the one cargo builds beside it.
    if let Ok(exe) = std::env::current_exe() {
        cash_awk::set_shell(exe.with_file_name(format!("cash{}", std::env::consts::EXE_SUFFIX)));
    }
    let code = cash_awk::run_awk(std::env::args_os());
    std::process::exit(code);
}
