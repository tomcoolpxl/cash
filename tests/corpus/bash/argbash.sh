# source: https://github.com/matejak/argbash/tree/2.11.0 (its `bin` and the generated scripts of `resources/examples`, unchanged, with its LICENSE, as the fixture directory `argbash`)
# desc: an argument-parser generator's own scripts, which it generated: argbash-init writing templates, and the example scripts' parsers taking short and long options, clustered and `=` values, booleans with --no-, a positional, the help, the version, and each kind of wrong command line refused with its status
a=argbash
e=$a/resources/examples
bash $a/bin/argbash --version
bash $a/bin/argbash-init --pos filename --opt unit --opt-bool verbose t1.m4; echo "init: $?"
cat t1.m4
bash $a/bin/argbash-init -s --mode minimal --opt level --hints t2.m4; echo "init: $?"
cat t2.m4; cat t2-parsing.m4 2>/dev/null
bash $a/bin/argbash-init --mode nonsense t3.m4; echo "status: $?"
printf 'hello world\n' > f.txt
bash $e/simple.sh --help; echo "status: $?"
bash $e/simple.sh f.txt
bash $e/simple.sh -u k --verbose f.txt
bash $e/simple.sh --unit=M --no-verbose f.txt
bash $e/simple.sh -ub f.txt
bash $e/simple.sh -- f.txt
bash $e/simple.sh; echo "status: $?"
bash $e/simple.sh f.txt extra; echo "status: $?"
bash $e/simple.sh -x f.txt; echo "status: $?"
bash $e/simple.sh --unit; echo "status: $?"
bash $e/simple.sh --version
bash $e/simple-standalone.sh -u k f.txt
bash $e/simple-standalone.sh --help
bash $e/simple-wrapper.sh --help
bash $e/simple-wrapper.sh f.txt; echo "status: $?"
mkdir d; printf abc > d/x; printf hello > d/y.txt
bash $e/simple-wrapper.sh d; echo "status: $?"
bash $e/simple-wrapper.sh --glob "*.txt" -u k --verbose d; echo "status: $?"
