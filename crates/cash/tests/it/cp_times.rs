//! The times `cp` gives a copy, as GNU's `cp` gives them: the present time, unless `-p`,
//! `-a` or `--preserve=timestamps` keeps the source's. Windows' `CopyFileExW` kept the
//! source's every time, and `cp -p` of a read-only file failed setting them.

#![allow(
    clippy::tests_outside_test_module,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::{Scratch, cash_command};

#[test]
fn a_copy_is_dated_now_unless_its_sources_times_are_kept() {
    let scratch = Scratch::new("cp-times");
    // Each line as Git Bash 5.3 gives it.
    let script = r#"echo x > old && touch -d 2020-01-02 old
cp old new
cp -p old kept
echo x > ro && touch -d 2020-01-02 ro && chmod a-w ro
cp -p ro rocopy; echo "p ro $?"
cp ro rocopy2; echo "ro $?"
cp -a old a
cp --preserve=timestamps old ts
for f in new kept rocopy rocopy2 a ts; do
  printf '%s %s %s\n' "$f" "$(date -r "$f" +%Y)" "$([ -w "$f" ] && echo rw || echo ro)"
done
chmod u+w ro rocopy rocopy2"#;
    let out = cash_command()
        .current_dir(scratch.path())
        .args(["--noprofile", "--norc", "-c", script])
        .output()
        .unwrap();
    let year = |stamp: &str| stamp != "2020";
    let stdout = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    let lines: Vec<Vec<&str>> = stdout.lines().map(|l| l.split(' ').collect()).collect();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(lines.len(), 8, "{stdout}{stderr}");
    assert_eq!(lines[0], ["p", "ro", "0"], "{stderr}");
    assert_eq!(lines[1], ["ro", "0"], "{stderr}");
    for line in &lines[2..] {
        let (name, stamp, write) = (line[0], line[1], line[2]);
        let now = matches!(name, "new" | "rocopy2");
        assert_eq!(year(stamp), now, "{name} dated {stamp}");
        let read_only = matches!(name, "rocopy" | "rocopy2");
        assert_eq!(write, if read_only { "ro" } else { "rw" }, "{name}");
    }
}
