//! Prints a page as `man` draws it, for whoever writes the pages:
//! `cargo run -p cash-man --example draw -- NAME [WIDTH] [--colour]`.
//!
//! With `--frame WIDTHxHEIGHT`, the browser's screen on NAME instead, as text:
//! `cargo run -p cash-man --example draw -- cd --frame 120x30`.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let colour = args.iter().any(|arg| arg == "--colour");
    let frame = args
        .iter()
        .position(|arg| arg == "--frame")
        .and_then(|at| args.get(at + 1))
        .and_then(|size| size.split_once('x'))
        .and_then(|(w, h)| Some((w.parse::<usize>().ok()?, h.parse::<usize>().ok()?)));
    let mut words = args
        .iter()
        .filter(|arg| !arg.starts_with("--") && !arg.contains('x'));
    let Some(name) = words.next() else {
        eprintln!("usage: draw NAME [WIDTH] [--colour] [--frame WIDTHxHEIGHT]");
        std::process::exit(2);
    };
    let manual = cash_man::manual();
    if let Some((width, height)) = frame {
        let mut browser = cash_man::browser::Browser::new(manual, Some(name), colour, |_| None);
        for line in browser.frame(width, height) {
            println!("{line}");
        }
        return;
    }
    let width = words
        .next()
        .and_then(|width| width.parse().ok())
        .unwrap_or(80);
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
