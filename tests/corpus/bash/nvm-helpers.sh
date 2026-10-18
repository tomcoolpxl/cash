# source: https://github.com/nvm-sh/nvm/blob/master/nvm.sh (the fixture `nvm.sh`, unchanged)
# desc: a 5,600-line real script that is one `{ }` command, sourced with --no-use: version comparing, normalizing and formatting, and the PATH helpers
# tags: cash-divergence (D3: $PWD is C:/..., whose colon splits nvm_strip_path's colon-separated list; TODO.md)
export NVM_DIR="$PWD/nvmdir"
mkdir -p "$NVM_DIR/versions/node"
. ./nvm.sh --no-use
echo "sourced: $(type -t nvm)"
for pair in '1.2.3 1.2.4' '10.0.0 9.9.9' 'v0.10.1 v0.9.20' '1.0.0 1.0.0' 'v20.1.0 v20.0.99'; do
    set -- $pair
    if nvm_version_greater "$1" "$2"; then g=yes; else g=no; fi
    if nvm_version_greater_than_or_equal_to "$1" "$2"; then ge=yes; else ge=no; fi
    echo "$1 > $2: $g, >= : $ge"
done
for v in 1 1.2 1.2.3 v4.5.6 v10 0.10.48 iojs-v1.0.0; do
    printf '%s -> [%s] [%s] [%s] [%s]\n' "$v" "$(nvm_normalize_version "$v")" \
        "$(nvm_format_version "$v")" "$(nvm_ensure_version_prefix "$v")" "$(nvm_strip_iojs_prefix "$v")"
done
for v in 1.2.3 v4 '' 1.2; do printf 'groups(%s)=%s\n' "$v" "$(nvm_num_version_groups "$v")"; done
for v in 1.2.3 v1.2.3 foo iojs node stable lts/* 1.x; do
    if nvm_is_valid_version "$v"; then echo "valid $v"; else echo "invalid $v"; fi
done
for kind in '' new old iojs; do
    d=$(nvm_version_dir $kind)
    echo "version dir ($kind): ${d#"$NVM_DIR"}"
done
p="/a/bin:$NVM_DIR/versions/node/v1.0.0/bin:/b/bin"
s=$(nvm_strip_path "$p" /bin)
echo "stripped: ${s//"$NVM_DIR"/NVM}"
c=$(nvm_change_path "$p" /bin "$NVM_DIR/versions/node/v2.0.0")
echo "changed: ${c//"$NVM_DIR"/NVM}"
nvm_tree_contains_path "$NVM_DIR" "$NVM_DIR/versions/node/x" && echo "tree contains"
nvm_tree_contains_path "$NVM_DIR" /elsewhere || echo "tree does not contain"
nvm_echo "echo works"
nvm_err "to standard error" 2>/dev/null
nvm_add_iojs_prefix v3.0.0
