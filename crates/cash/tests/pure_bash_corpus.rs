//! Pure-Bash Programs, Esoteric Quirks, and Computational Compatibility Suite.
//!
//! Verifies that `cash` can execute complex, Turing-complete algorithms,
//! esoteric parameter transformations, arcane syntax hacks, and benchmark scripts
//! written purely in Bash without external binaries or Linux `/proc`.

#![cfg(windows)]
#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::needless_raw_string_hashes,
    reason = "integration tests abort loudly on unexpected failures"
)]

use std::process::Command;

const CASH: &str = env!("CARGO_BIN_EXE_cash");

struct Output {
    stdout: String,
    stderr: String,
    code: i32,
}

fn cash(script: &str) -> Output {
    let out = Command::new(CASH)
        .args(["-c", script])
        .output()
        .expect("failed to execute cash");
    Output {
        stdout: String::from_utf8_lossy(&out.stdout).trim_end().to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).trim_end().to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

// ---------------------------------------------------------------------------
// 1. Complex Pure-Bash Programs & Turing-Complete Interpreters
// ---------------------------------------------------------------------------

#[test]
fn pure_bash_brainfuck_interpreter() {
    // Pure Bash Brainfuck interpreter executing nested bracket loops,
    // tape memory manipulation, arithmetic pointer bounds, and ASCII emission.
    // Program prints 'H' (72) then 'i' (105).
    let script = r###"
program="++++++++[>+++++++++<-]>.>+++++++[>+++++++++++++++<-]>."
program=${program//[^><+-.,[]]}
declare -A tape
declare -a stack
for (( i = ptr = 0; i < ${#program}; ++i )); do
    case ${program:i:1} in
        ">") (( ++ptr )) ;;
        "<") (( --ptr )) ;;
        "+") (( ++tape[$ptr] )); (( tape[$ptr] > 255 )) && tape[$ptr]=0 ;;
        "-") (( --tape[$ptr] )); (( tape[$ptr] < 0 )) && tape[$ptr]=255 ;;
        ".") printf -v f %x "${tape[$ptr]}"; printf %b "\x$f" ;;
        "[")
            if (( tape[$ptr] )); then
                stack+=("$i")
            else
                for (( depth = 1; depth > 0 && ++i; )); do
                    case ${program:i:1} in
                        "[") (( ++depth )) ;;
                        "]") (( --depth )) ;;
                    esac
                done
            fi
            ;;
        "]")
            (( _ = ${#stack[@]} ))
            if (( tape[$ptr] )); then
                (( i = stack[_-1] ))
            else
                unset "stack[_-1]"
            fi
            ;;
    esac
done
"###;
    let out = cash(script);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "Hi");
}

#[test]
fn pure_bash_json_parser() {
    // Pure Bash JSON tokenizer and parser using single-character stream consumption,
    // pattern matching, and associative arrays (nosj architecture).
    let script = r###"
json_data='{"shell":"cash","version":"0.8.0","target":"windows"}'

tokenize() {
    local j str
    while read -rN 1; do
        case $REPLY in
            [\{\}\[\],])
                [[ $str ]] && j+=$REPLY
                [[ $str ]] || { tokens+=("$j" "$REPLY"); j=; }
            ;;
            :)
                [[ $str ]] && j+=:
                [[ $str ]] || j+="\ "
            ;;
            [[:space:]])
                [[ $str ]] && j+=$REPLY
            ;;
            [\"\x27])
                [[ $str ]] && str= || str=1
                [[ ${j: -1} == \\ ]] && { str=1; j+=$REPLY; }
            ;;
            *) j+=$REPLY ;;
        esac
    done
}

parse() {
    local i key val
    declare -A parsed
    for ((i=0;i<${#tokens[@]};i++)); do
        case ${tokens[i]} in
            *\\\ *)
                key=${tokens[i]/\\ *}
                val=${tokens[i]/*\\ }
                key=${key#\"}; key=${key%\"}
                key=${key#\ }; key=${key%\ }
                val=${val#\"}; val=${val%\"}
                parsed["$key"]="$val"
            ;;
        esac
    done
    echo "shell=${parsed[shell]} ver=${parsed[version]} target=${parsed[target]}"
}

echo -n "$json_data" | {
    tokens=()
    tokenize
    parse
}
"###;
    let out = cash(script);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "shell=cash ver=0.8.0 target=windows");
}

#[test]
fn pure_bash_rule110_cellular_automaton() {
    // Stephen Wolfram's Turing-complete Rule 110 elementary cellular automaton
    // implemented via bitwise arithmetic and modulo wrap-around.
    let script = r###"
state=(0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 1)
len=${#state[@]}
for ((gen=0; gen<4; gen++)); do
    line=""
    for ((i=0; i<len; i++)); do
        (( state[i] )) && line+="#" || line+="."
    done
    echo "$line"
    next=()
    for ((i=0; i<len; i++)); do
        left=${state[(i - 1 + len) % len]}
        center=${state[i]}
        right=${state[(i + 1) % len]}
        pattern=$(( (left << 2) | (center << 1) | right ))
        bit=$(( (110 >> pattern) & 1 ))
        next[i]=$bit
    done
    state=("${next[@]}")
done
"###;
    let out = cash(script);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0], "...................#");
    assert_eq!(lines[1], "..................##");
    assert_eq!(lines[2], ".................###");
    assert_eq!(lines[3], "................##.#");
}

#[test]
fn pure_bash_recursive_tower_of_hanoi() {
    let script = r###"
hanoi() {
    local n=$1 from=$2 to=$3 aux=$4
    if (( n == 1 )); then
        echo "$from->$to"
        return
    fi
    hanoi $((n - 1)) "$from" "$aux" "$to"
    echo "$from->$to"
    hanoi $((n - 1)) "$aux" "$to" "$from"
}
hanoi 3 A C B
"###;
    let out = cash(script);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "A->C\nA->B\nC->B\nA->C\nB->A\nB->C\nA->C"
    );
}

#[test]
fn pure_bash_sieve_of_eratosthenes() {
    let script = r###"
sieve() {
    local limit=$1
    declare -a is_prime
    for ((i=2; i<=limit; i++)); do
        is_prime[i]=1
    done
    for ((p=2; p*p<=limit; p++)); do
        if (( is_prime[p] == 1 )); then
            for ((i=p*p; i<=limit; i+=p)); do
                is_prime[i]=0
            done
        fi
    done
    local primes=()
    for ((i=2; i<=limit; i++)); do
        if (( is_prime[i] == 1 )); then
            primes+=($i)
        fi
    done
    echo "${primes[*]}"
}
sieve 30
"###;
    let out = cash(script);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "2 3 5 7 11 13 17 19 23 29");
}

#[test]
fn pure_bash_recursive_quicksort() {
    let script = r###"
quicksort() {
    local -a arr=("$@")
    if (( ${#arr[@]} <= 1 )); then
        echo "${arr[@]}"
        return
    fi
    local pivot="${arr[0]}"
    local -a left=() right=()
    for ((i=1; i<${#arr[@]}; i++)); do
        if (( arr[i] < pivot )); then
            left+=("${arr[i]}")
        else
            right+=("${arr[i]}")
        fi
    done
    local sorted_left sorted_right
    if (( ${#left[@]} > 0 )); then
        sorted_left=$(quicksort "${left[@]}")
    fi
    if (( ${#right[@]} > 0 )); then
        sorted_right=$(quicksort "${right[@]}")
    fi
    echo ${sorted_left} $pivot ${sorted_right}
}
quicksort 42 17 88 5 99 23 1
"###;
    let out = cash(script);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "1 5 17 23 42 88 99");
}

// ---------------------------------------------------------------------------
// 2. Pure-Bash-Bible Idioms & String Manipulations
// ---------------------------------------------------------------------------

#[test]
fn pure_bash_bible_string_and_color_manipulations() {
    let script = r###"
trim_string() {
    : "${1#"${1%%[![:space:]]*}"}"
    : "${_%"${_##*[![:space:]]}"}"
    printf '%s\n' "$_"
}
hex_to_rgb() {
    : "${1/\#}"
    ((r=16#${_:0:2},g=16#${_:2:2},b=16#${_:4:2}))
    printf '%s\n' "$r $g $b"
}
rgb_to_hex() {
    printf '#%02x%02x%02x\n' "$1" "$2" "$3"
}
urldecode() {
    local url_encoded="${1//+/ }"
    printf '%b\n' "${url_encoded//%/\\x}"
}

trimmed=$(trim_string "   hello world   ")
echo "trimmed=[$trimmed]"
rgb=$(hex_to_rgb "#FFA500")
echo "rgb=$rgb"
hex=$(rgb_to_hex 255 165 0)
echo "hex=$hex"
decoded=$(urldecode "cool%20cash%21")
echo "decoded=$decoded"
"###;
    let out = cash(script);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "trimmed=[hello world]\nrgb=255 165 0\nhex=#ffa500\ndecoded=cool cash!"
    );
}

#[test]
fn pure_bash_nameref_mutation() {
    let script = r###"
to_upper() {
    local -n ptr=$1
    ptr=${ptr^^}
}
val="lowercase text"
to_upper val
echo "val=$val"
"###;
    let out = cash(script);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "val=LOWERCASE TEXT");
}

// ---------------------------------------------------------------------------
// 3. Esoteric Parameter Transformations & Arcane Hacks
// ---------------------------------------------------------------------------

#[test]
fn esoteric_bash_parameter_transformations() {
    let script = r###"
text="it's a string"
echo "Q: ${text@Q}"
escaped="line1\nline2"
echo "E: ${escaped@E}"
declare -i count=123
echo "A: ${count@A}"
echo "a: ${count@a}"
"###;
    let out = cash(script);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines[0], "Q: 'it'\\''s a string'");
    assert_eq!(lines[1], "E: line1");
    assert_eq!(lines[2], "line2");
    assert_eq!(lines[3], "A: count='123'");
    assert_eq!(lines[4], "a: i");
}

#[test]
fn esoteric_bash_arithmetic_quirks() {
    let script = r###"
# 1. Multi-base arithmetic (binary, octal, hex, base64)
(( val = 2#1010 + 8#12 + 16#1A + 64#A ))
echo "base_sum=$val"

# 2. Comma operator sequence
(( x = 2, y = x * 4, z = y + 3 ))
echo "comma: x=$x y=$y z=$z"

# 3. Nested ternary evaluation
(( res = 10 > 5 ? (30 < 20 ? 111 : 222) : 333 ))
echo "ternary=$res"

# 4. Bitwise shifts & masks
(( mask = (1 << 6) | (1 << 2) ^ 0x0A ))
echo "mask=$mask"

# 5. Pre/post increment inside array subscriptions
declare -a arr
idx=0
(( arr[idx++] = 100, arr[idx++] = 200, arr[idx] = 300 ))
echo "arr=${arr[*]} (idx=$idx)"
"###;
    let out = cash(script);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "base_sum=82\ncomma: x=2 y=8 z=11\nternary=222\nmask=78\narr=100 200 300 (idx=2)"
    );
}

#[test]
fn esoteric_bash_parameter_expansion_quirks() {
    let script = r###"
# 1. Negative slice offsets
v="0123456789"
echo "neg_slice: [${v: -5}] [${v: -5:3}]"

# 2. Pattern anchor substitutions
path="/usr/local/bin/bash"
echo "anchor_start: ${path/#\/usr\/local/C:}"
echo "anchor_end: ${path/%bash/cash}"

# 3. Indirect variable prefix listing
MYTEST_VAR_1="first"
MYTEST_VAR_2="second"
echo "prefix_list: ${!MYTEST_VAR_*}"

# 4. Sparse array keys
declare -a sparse
sparse[10]="ten"
sparse[50]="fifty"
echo "sparse_keys: ${!sparse[@]}"
"###;
    let out = cash(script);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "neg_slice: [56789] [567]\nanchor_start: C:/bin/bash\nanchor_end: /usr/local/bin/cash\nprefix_list: MYTEST_VAR_1 MYTEST_VAR_2\nsparse_keys: 10 50"
    );
}

// ---------------------------------------------------------------------------
// 4. Performance & Loop Benchmark Parity
// ---------------------------------------------------------------------------

#[test]
fn pure_bash_loop_benchmark() {
    // 2,000 iterations of pure bash arithmetic loop to verify throughput,
    // memory stability, and arithmetic accumulator correctness.
    let script = r###"
sum=0
for (( i = 1; i <= 2000; i++ )); do
    (( sum += i ))
done
echo "triangular_2000=$sum"
"###;
    let out = cash(script);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    // n*(n+1)/2 = 2000 * 2001 / 2 = 2,001,000
    assert_eq!(out.stdout, "triangular_2000=2001000");
}
