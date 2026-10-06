# column cases, run under util-linux column (the oracle) and under cash's builtin. Output
# is shown through od with spaces turned into underscores first, so padding, tabs, line
# endings and missing final newlines are all visible; no test input contains `_`.
# column_cases.out is util-linux 2.42.3's output.
#
# Regenerate the golden file (WSL):
#   bash column_cases.sh > column_cases.out 2>&1

exec 2>&1
export COLUMNS=80
dir=$(mktemp -d)
cd "$dir" || exit 1
show() { tr ' ' '_' | od -An -c | sed 's/  */ /g; s/ *$//'; }

echo "== fill: columns first, 80 wide"; printf 'a\nbb\nccc\ndddd\neeeee\nffffff\n' | column | show
echo "== fill: -c 20"; printf 'a\nbb\nccc\ndddd\neeeee\nffffff\n' | column -c 20 | show
echo "== fill: -x fills rows first"; printf 'a\nbb\nccc\ndddd\neeeee\nffffff\n' | column -x -c 20 | show
echo "== fill: 30 numbers in 40 columns"; seq 1 30 | column -c 40 | show
echo "== fill: 30 numbers in 40 columns, -x"; seq 1 30 | column -x -c 40 | show
echo "== fill: an item of 8 rounds the tab stop up"; printf 'aaaaaaaa\nb\nc\n' | column -c 32 | show
echo "== fill: items wider than the width go one per line"; printf 'aaaaaaaaaaaa\nb\nc\n' | column -c 10 | show
echo "== fill: -c 0 is unlimited, one per line"; printf 'a\nb\n' | column -c 0 | show
echo "== fill: COLUMNS decides the width"; printf 'aaaa\nbbbb\ncccc\ndddd\n' | COLUMNS=30 column | show
echo "== fill: an unusable COLUMNS means 80"; printf 'a\nb\n' | COLUMNS=abc column | show
echo "== fill: -S uses spaces, not tabs"; printf 'a\nbb\nccc\n' | column -S 1 -c 20 | show
echo "== fill: -S 2 in 10 columns"; printf 'a\nb\nc\nd\ne\nf\ng\n' | column -S 2 -c 10 | show
echo "== fill: -S 2, -x"; printf 'aaa\nbbb\nccc\nddd\neee\n' | column -S 2 -x -c 15 | show
echo "== fill: -S lets the gap fit the remainder"; printf 'aaaa\nbbbb\n' | column -S 2 -c 5 | show
echo "== fill: empty lines are dropped"; printf 'a\n\nb\n' | column -c 20 | show
echo "== fill: -L keeps empty lines"; printf 'a\n\nb\n' | column -L -c 20 | show
echo "== fill: a line of spaces is empty"; printf 'a\n   \nb\n' | column -c 20 | show
echo "== fill: leading spaces stay in the item"; printf '  a\n b\nc\nd\ne\nf\n' | column -S 2 -c 12 | show
echo "== fill: CJK items count two cells"; printf '日本\nab\nc\nd\n' | column -c 16 | show
echo "== fill: CJK items, -S"; printf '日本\nab\nc\nd\n' | column -S 2 -c 16 | show
echo "== fill: an escape sequence has no width"; printf 'a\033[1mb\nc\nd\ne\n' | column -S 2 -c 20 | show
echo "== fill: no final newline"; printf 'a\nb' | column -c 20 | show
echo "== fill: CRLF lines"; printf 'a\r\nbb\r\n' | column -c 20 | show
echo "== fill: a tab inside an item"; printf 'a\tb\nc\nd\ne\nf\n' | column -c 20 | show
echo "== fill: -o and -s have no effect"; printf 'a,b\nc\n' | column -s , -o X -c 20 | show
echo "== fill: empty input"; printf '' | column | show; printf '' | column; echo "rc=$?"

echo "== table: whitespace separates"; printf 'a bb ccc\ndddd e f\n' | column -t | show
echo "== table: runs of blanks are one separator"; printf 'a   bb\t\tccc\n  dddd e f  \n' | column -t | show
echo "== table: ragged rows"; printf 'a bb ccc\nd\n' | column -t | show
echo "== table: a short first row"; printf 'a\nd ee fff\n' | column -t | show
echo "== table: -s ,"; printf 'a,bb,ccc\ndddd,e,f\n' | column -t -s , | show
echo "== table: -s keeps empty cells"; printf 'a,,ccc\n,e,\n' | column -t -s , | show
echo "== table: -s and a trailing separator"; printf 'a,b,\ncc,dd,ee\n' | column -t -s , | show
echo "== table: -s with several characters"; printf 'a,b;c\nd;e,f\n' | column -t -s ',;' | show
echo "== table: -s tab"; printf 'a\tbb\tccc\nd\te\tf\n' | column -t -s "$(printf '\t')" | show
echo "== table: -s a space splits every space"; printf 'a  b\nc d\n' | column -t -s ' ' | show
echo "== table: -s a multibyte character"; printf 'a→b→c\nd→e→f\n' | column -t -s '→' | show
echo "== table: the last -s wins"; printf 'a,b;c\n' | column -t -s ';' -s ',' | show
echo "== table: -o"; printf 'a bb\nccc d\n' | column -t -o ' | ' | show
echo "== table: -o empty"; printf 'a bb\nccc d\n' | column -t -o '' | show
echo "== table: -N names the columns"; printf 'a bb\nccc d\n' | column -t -N X,YY | show
echo "== table: -N with fewer names than columns"; printf 'a bb c\nccc d e\n' | column -t -N X,YY | show
echo "== table: -N with more names than columns"; printf 'a bb\nccc d\n' | column -t -N X,YY,ZZZ | show
echo "== table: a header wider than its data"; printf 'a b\n' | column -t -N LONGNAME,Y | show
echo "== table: -N drops empty names"; printf 'a b\n' | column -t -N ,y | show
echo "== table: -d hides the header"; printf 'a bb\nccc d\n' | column -t -N X,YY -d | show
echo "== table: -R right-aligns a column"; printf 'a bb\nccc d\n' | column -t -R 2 | show
echo "== table: -R on the first column"; printf 'a bb\nccc d\n' | column -t -R 1 | show
echo "== table: -R by name, header too"; printf 'aaa b\n' | column -t -N X,Y -R X | show
echo "== table: -R on the last column pads it"; printf 'a bbb\n' | column -t -N X,Y -R Y | show
echo "== table: -R 0 is every column"; printf 'a bb\nccc d\n' | column -t -R 0 | show
echo "== table: -R -1 is the last column"; printf 'a bb\nccc d\n' | column -t -R -1 | show
echo "== table: -R 2-3 is a range"; printf 'a bb c\nccc d e\n' | column -t -R 2-3 | show
echo "== table: -R with a ragged last column"; printf 'a bb c\nddd e\n' | column -t -R 3 | show
echo "== table: -R with CJK"; printf '日本 a\nb c\n' | column -t -R 1 | show
echo "== table: -H hides a column"; printf 'a bb c\nccc d e\n' | column -t -H 2 | show
echo "== table: -H by name"; printf 'a bb c\nccc d e\n' | column -t -N X,Y,Z -H Y | show
echo "== table: -H - hides unnamed columns"; printf 'a bb c\nccc d e\n' | column -t -N x -H - | show
echo "== table: -O orders columns"; printf 'a bb c\nccc d e\n' | column -t -O 3,1 | show
echo "== table: -O by name"; printf 'a bb c\nccc d e\n' | column -t -N X,Y,Z -O Z,X | show
echo "== table: -l limits the columns"; printf 'a   bb  ccc dd\ne f g h\n' | column -t -l 2 | show
echo "== table: -l with -s keeps the rest verbatim"; printf 'a,bb,ccc,dd\ne,f,g,h\n' | column -t -l 2 -s , | show
echo "== table: -L keeps empty lines"; printf 'a bb\n\nccc d\n' | column -t -L | show
echo "== table: empty lines dropped without -L"; printf 'a bb\n\nccc d\n' | column -t | show
echo "== table: -K takes the first row as header"; printf 'NAME SIZE\na 1\nbb 22\n' | column -t -K | show
echo "== table: -K with -R by name"; printf 'X Y\na bbb\n' | column -t -K -R Y | show
echo "== table: -K with -d"; printf 'NAME SIZE\na 1\n' | column -t -K -d | show
echo "== table: -m fills the width"; printf 'a b c\nccc d e\n' | column -t -m -c 20 | show
echo "== table: -m with -N"; printf 'a b\nccc d\n' | column -t -m -c 20 -N X,Y | show
echo "== table: CJK aligned by cells"; printf '日本 a\nb c\n' | column -t | show
echo "== table: accents"; printf 'é a\nbb c\n' | column -t | show
echo "== table: a combining mark has no width"; printf 'e\xcc\x81 a\nbb c\n' | column -t | show
echo "== table: an emoji sequence"; printf '👨‍👩 c\nddd e\n' | column -t | show
echo "== table: control characters have no width"; printf 'a\001b c\nddd e\n' | column -t | show
echo "== table: an escape sequence has no width"; printf 'a\033[1mb c\nddddd e\n' | column -t | show
echo "== table: invalid UTF-8"; printf 'a\xffb c\nd e\n' | column -t | show
echo "== table: CRLF lines"; printf 'a bb\r\nccc d\r\n' | column -t | show
echo "== table: CRLF lines with -s"; printf 'a,bb\r\nccc,d\r\n' | column -t -s , | show
echo "== table: no final newline"; printf 'a b\nc d' | column -t | show
echo "== table: one cell"; printf 'abc\n' | column -t | show
echo "== table: a tab inside a cell with -s"; printf 'a\tb,c\nd,e\n' | column -t -s , | show
echo "== table: wider than the width"; printf 'aaaaaaaaaaaa b\ncc dd\n' | column -t -c 10 | show
echo "== table: empty input"; printf '' | column -t | show; printf '' | column -t; echo "rc=$?"
echo "== table: only empty lines"; printf '\n\n' | column -t | show
echo "== table: only empty lines with -L"; printf '\n\n' | column -t -L | show
echo "== table: -s with a line of separators"; printf 'a,b\n,,\nc,d\n' | column -t -s , | show
echo "== table: -t -x cannot combine"; printf 'a b\n' | column -t -x; echo "rc=$?"
echo "== table: -J -x cannot combine"; printf 'a b\n' | column -J -x -N a,b; echo "rc=$?"
echo "== table: -K -N cannot combine"; printf 'a b\n' | column -t -K -N x,y; echo "rc=$?"
echo "== table: -R needs -t"; printf 'a b\n' | column -R 1; echo "rc=$?"
echo "== table: -N needs -t"; printf 'a b\n' | column -N x,y; echo "rc=$?"
echo "== table: -n needs -t"; printf 'a b\n' | column -n x; echo "rc=$?"
echo "== table: -R of an unknown column"; printf 'a b\n' | column -t -R 5; echo "rc=$?"
echo "== table: -R of an unknown name"; printf 'a b\n' | column -t -N x,y -R z; echo "rc=$?"
echo "== table: -H of an unknown column"; printf 'a b\n' | column -t -H z; echo "rc=$?"
echo "== table: -O of an unknown column"; printf 'a b\n' | column -t -O 3; echo "rc=$?"
echo "== table: -l 0"; printf 'a\n' | column -t -l 0; echo "rc=$?"
echo "== table: -l x"; printf 'a\n' | column -t -l x; echo "rc=$?"

echo "== json: -J -n table -N a,b"; printf 'a bb\nccc d\n' | column -J -n tbl -N a,b
echo "== json: the default table name"; printf 'a bb\nccc d\n' | column -J -N a,b
echo "== json: -J needs -N"; printf 'a bb\nccc d\n' | column -J; echo "rc=$?"
echo "== json: quotes and backslashes"; printf 'x"y a\\b\n' | column -J -N a,b
echo "== json: control characters"; printf 'a\001b\tc\n' | column -J -N a,b -s "$(printf '\t')"
echo "== json: UTF-8"; printf 'é 日\n' | column -J -N a,b
echo "== json: keys are lowercased"; printf 'a b\n' | column -J -N Name,SIZE -n Tbl
echo "== json: ragged rows give null"; printf 'a bb c\nd\n' | column -J -N a,b,c
echo "== json: more names than cells"; printf 'a\n' | column -J -N a,b,c
echo "== json: an empty cell is null"; printf 'a,,c\n' | column -J -N a,b,c -s ,
echo "== json: -L gives a row of nulls"; printf 'a b\n\nc d\n' | column -J -N a,b -L
echo "== json: -H"; printf 'a bb c\n' | column -J -N a,b,c -H b
echo "== json: -O"; printf 'a bb c\n' | column -J -N a,b,c -O c,a
echo "== json: -K"; printf 'A B\n1 2\n3\n' | column -J -K
echo "== json: -K with -n"; printf 'A B\n1 2\n' | column -J -K -n t
echo "== json: an unnamed column is an error"; printf 'a bb c\n' | column -J -N x,y; echo "rc=$?"
echo "== json: -H - hides the unnamed columns"; printf 'a bb c\nccc d e\n' | column -J -N x -H -
echo "== json: CRLF lines"; printf 'a b\r\n' | column -J -N a,b
echo "== json: empty input"; printf '' | column -J -N a,b; echo "rc=$?"

echo "== files"; printf 'a b\n' > a; printf 'ccc d\n' > b; column -t a b | show
echo "== files: -K applies to the first only"; printf 'A B\n1 2\n' > k1; printf 'C D\n3 4\n' > k2; column -t -K k1 k2 | show
echo "== files: a missing file is reported, the rest shown"; column -t a nosuch b; echo "rc=$?"
echo "== files: a missing file in fill mode"; column a nosuch; echo "rc=$?"
echo "== files: only a missing file"; column nosuch; echo "rc=$?"
echo "== files: dash is a file name"; printf 'x y\n' | column -t -; echo "rc=$?"
echo "== files: options after the file"; column a -t | show
echo "== files: -- ends the options"; column -- -t 2>&1; echo "rc=$?"
echo "== files: an empty file"; printf '' > e; column -t e | show; column e; echo "rc=$?"

echo "== options: a bad option"; column -Z; echo "rc=$?"
echo "== options: a bad long option"; column --nope; echo "rc=$?"
echo "== options: an ambiguous long option"; column --table-c=x 2>&1 | head -1; column --table-c=x 2>/dev/null; echo "rc=$?"
echo "== options: a long option prefix"; printf 'a,b\n' | column --tabl --sep , | show
echo "== options: --output-width=20"; printf 'a\nbb\n' | column --output-width=20 | show
echo "== options: --json, --table-columns, --table-name"; printf 'a b\n' | column --json --table-columns=a,b --table-name=n
echo "== options: --keep-empty-lines"; printf 'a\n\nb\n' | column --keep-empty-lines -c 20 | show
echo "== options: --table-empty-lines"; printf 'a\n\nb\n' | column --table-empty-lines -c 20 | show
echo "== options: --fillrows"; printf 'a\nb\nc\n' | column --fillrows -c 20 | show
echo "== options: --use-spaces"; printf 'a\nbb\n' | column --use-spaces 3 -c 20 | show
echo "== options: clustered -ts,"; printf 'a,b\n' | column -ts, | show
echo "== options: --json=x takes no argument"; column --json=x; echo "rc=$?"
echo "== options: --table-columns needs an argument"; column --table-columns; echo "rc=$?"
echo "== options: -c needs an argument"; column -c; echo "rc=$?"
echo "== options: -c abc"; printf 'a\n' | column -c abc; echo "rc=$?"
echo "== options: -c -5"; printf 'a\n' | column -c -5; echo "rc=$?"
echo "== options: -c 5x"; printf 'a\n' | column -c 5x; echo "rc=$?"
echo "== options: -c unlimited"; printf 'a\nb\nc\n' | column -c unlimited | show
echo "== options: -S x"; printf 'a\n' | column -S x; echo "rc=$?"
echo "== options: --color is accepted"; printf 'a b\n' | column -t --color=never | show; printf 'a b\n' | column -t --color | show
echo "== options: --color=nope"; printf 'a b\n' | column -t --color=nope; echo "rc=$?"
echo "== options: -h"; column -h | head -3; column --help >/dev/null; echo "rc=$?"
echo "== options: -h before a bad option"; column -h -Z | head -3; column -h -Z >/dev/null; echo "rc=$?"
echo "== options: -V"; column -V; echo "rc=$?"

cd / && rm -rf "$dir"
