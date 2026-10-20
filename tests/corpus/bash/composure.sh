# source: https://github.com/erichs/composure/tree/1.4.0 (`composure.sh`, unchanged, with its LICENSE, as the fixture directory `composure`)
# desc: composure's function metadata: a function that describes itself with about, param, example, group, author and version, which run as no-ops; reference and glossary read the keywords back out of `typeset -f`, metafor picks one keyword, and write prints the composed functions as a script; its own functions persisted under XDG_DATA_HOME
# timeout: 30
export HOME="$PWD/home" XDG_DATA_HOME="$PWD/home/data" SHELL=/usr/bin/bash
mkdir -p "$HOME" "$XDG_DATA_HOME/composure"
. composure/composure.sh
# composure asks ps for the shell's name, which Git's ps cannot say.
_shell() { echo bash; }
greet () {
  about 'says hello to someone'
  param '1: who to greet'
  param '2: how often'
  example '$ greet world'
  example '$ greet world 2'
  group 'demo'
  author 'tester'
  version '1.0'
  typeset i
  for ((i = 0; i < ${2:-1}; i++)); do
    echo "hello ${1:-you}"
  done
}
greet world 2
echo "--- reference"
reference greet
echo "--- metafor"
typeset -f greet | metafor about
typeset -f greet | metafor param
typeset -f greet | metafor example
echo "--- glossary"
glossary demo
glossary composure
echo "--- cite"
cite url username
url 'https://example.com' && username alice && echo "cited keywords run as no-ops"
echo "--- typeset -f greet"
typeset -f greet
echo "--- write"
write greet
echo "--- persisted"
ls "$XDG_DATA_HOME/composure" | sort
