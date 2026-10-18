//! Prints a page as `man` draws it, for whoever writes the pages:
//! `cargo run -p cash-man --example draw -- NAME [WIDTH] [--colour]`.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let colour = args.iter().any(|arg| arg == "--colour");
    let mut words = args.iter().filter(|arg| !arg.starts_with("--"));
    let Some(name) = words.next() else {
        eprintln!("usage: draw NAME [WIDTH] [--colour]");
        std::process::exit(2);
    };
    let width = words
        .next()
        .and_then(|width| width.parse().ok())
        .unwrap_or(80);
    let manual = cash_man::manual();
    let Some(page) = manual.find(name) else {
        eprintln!("draw: no page for {name}");
        std::process::exit(1);
    };
    for problem in page.problems() {
        eprintln!("draw: {}.md: {problem}", page.file);
    }
    print!(
        "{}",
        cash_man::render::draw(page, manual, width).text(colour)
    );
}
