//! How `read` splits a line into its variables, and `mapfile`'s records, as Bash 5.3 does.
//!
//! Found by Bash's own test suite (2026-10-20): `IFS=": " read x y` of `a :b` gave `:b`,
//! the blank and the colon taken for two delimiters, in 832 of its 6856 generated IFS
//! cases; and a backslash-escaped delimiter split the line all the same, as the escape was
//! taken out before the splitting.
#![cfg(windows)]
#![allow(
    clippy::tests_outside_test_module,
    clippy::unwrap_used,
    reason = "integration assertions"
)]

use crate::common::cash_command;

fn output(script: &str) -> (i32, String) {
    let result = cash_command()
        .args(["--noprofile", "--norc", "-c", script])
        .output()
        .unwrap();
    let stdout = String::from_utf8(result.stdout)
        .unwrap()
        .replace("\r\n", "\n");
    (
        result.status.code().unwrap_or(-1),
        stdout.trim_end().to_string(),
    )
}

#[test]
fn read_splits_at_ifs_and_escapes_as_bash_does() {
    // A delimiter is IFS whitespace, at most one other IFS character, and IFS whitespace
    // again; an escaped character is no delimiter; `REPLY` and `-a` take the escapes out
    // too. `mapfile -d ''` kept the NUL in each element, which no Bash value holds.
    let script = r#"IFS=": " read x y <<< "a :b"; echo "[$x][$y]"
IFS=": " read x y <<< "a : b : "; echo "[$x][$y]"
read x y <<< 'a\ b c'; echo "[$x][$y]"
echo " a  b\ " | (read x y; echo "-$x-$y-")
IFS=: read x y <<< 'p\:q:r'; echo "[$x][$y]"
read -r x y <<< 'a\ b c'; echo "[$x][$y]"
read <<< ' r\ e '; echo "[$REPLY]"
read -a arr <<< 'one\ two three'; echo "${#arr[@]} [${arr[0]}]"
printf 'abc\0def\0' | { mapfile -d '' A; echo "${#A[0]} ${#A[1]}"; }"#;
    assert_eq!(
        output(script),
        (
            0,
            "[a][b]\n[a][b]\n[a b][c]\n-a-b -\n[p:q][r]\n[a\\][b c]\n[ r e ]\n2 [one two]\n3 3"
                .into()
        )
    );
}
