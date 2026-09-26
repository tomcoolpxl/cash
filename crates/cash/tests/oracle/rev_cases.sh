# rev cases, run under util-linux rev (the oracle) and under cash's builtin. Output is
# shown through od so line endings, NULs and missing final newlines are visible.
# rev_cases.out is util-linux 2.42.3's output.
#
# Regenerate the golden file (WSL):
#   bash rev_cases.sh > rev_cases.out 2>&1

exec 2>&1
dir=$(mktemp -d)
cd "$dir" || exit 1
show() { od -An -c | sed 's/  */ /g; s/ *$//'; }

echo "== plain lines"; printf 'abc\nxyz\n' | rev | show
echo "== no final newline"; printf 'abc\nxy' | rev | show
echo "== CRLF keeps its CR at the end"; printf 'abc\r\nxy\r\n' | rev | show
echo "== empty lines"; printf '\n\nab\n' | rev | show
echo "== empty input"; printf '' | rev | show; echo "rc=$?"
echo "== UTF-8"; printf 'h\303\251llo w\303\266rld\n' | rev | show
echo "== the domain idiom"; echo www.example.co.uk | rev | cut -d. -f1-2 | rev
echo "== files"; printf 'one\n' > a; printf 'two\nthree\n' > b; rev a b | show
echo "== missing file"; rev a nosuch b 2>&1 | show; rev nosuch 2>/dev/null; echo "rc=$?"
echo "== -0"; printf 'ab\0cd\0' | rev -0 | show
echo "== --zero, no final NUL"; printf 'ab\0cd' | rev --zero | show
echo "== dash is stdin"; printf 'xy\n' | rev - | show
echo "== bad option"; rev -Z 2>&1 | head -1; rev -Z 2>/dev/null; echo "rc=$?"

cd / && rm -rf "$dir"
