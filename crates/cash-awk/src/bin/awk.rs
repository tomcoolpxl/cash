fn main() {
    let code = cash_awk::run_awk(std::env::args_os());
    std::process::exit(code);
}
