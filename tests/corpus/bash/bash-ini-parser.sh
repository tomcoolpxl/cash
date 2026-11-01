# source: https://github.com/rudimeier/bash_ini_parser/tree/8fb95e3 (`read_ini.sh`, `test/` and LICENSE, unchanged, as the fixture directory `bash_ini_parser`)
# desc: bash_ini_parser at 8fb95e3, the INI file reader that evals each key into a shell variable: its nine tests, each a script reading an INI file and printing the variables, compared with the output the test expects as its runner compares them (leading and trailing blanks, comments, quoted and empty values, sections, a prefix, booleans, and the quoting of quotes); the outputs are written beside the test directory, since one INI runs `ls .`
cd bash_ini_parser/test || exit 1
mkdir -p ../../out
for test in test[1-9].sh; do
  name=${test%.sh}
  bash "$test" > "../../out/$name.out" 2>&1
  echo "== $name: status $?"
  if diff "../../out/$name.out" "$name.out.correct"; then echo "as expected"; fi
done
