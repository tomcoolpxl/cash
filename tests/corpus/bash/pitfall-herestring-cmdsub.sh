# source: https://mywiki.wooledge.org/BashPitfalls#while_.2026_done_.3C.3C.3C_.22.24.28foo.29.22
# desc: <<< "$(foo)" strips trailing newlines and adds one; < <(foo) keeps them
foo() { printf 'a\nb\n\n\n'; }
n=0; while IFS= read -r l; do n=$((n+1)); done <<< "$(foo)"; echo "herestring lines: $n"
n=0; while IFS= read -r l; do n=$((n+1)); done < <(foo); echo "procsub lines: $n"
