# source: https://github.com/todotxt/todo.txt-cli/tree/v2.14.0 (`todo.sh` and `todo.cfg`, unchanged, with its LICENSE, as the fixture directory `todo-txt`)
# desc: a todo-list program of one bash script: tasks added one and many at a time, listed, filtered by term, project, context and priority, prioritised and back, appended, prepended, replaced, moved between files, done and archived, deleted with and without a term, deduplicated, its add-on directory, help and the version; plain output, the dates it writes left out
# timeout: 60
export HOME="$PWD/home" TODO_DIR="$PWD/todo-data"
mkdir -p "$HOME" "$TODO_DIR"
t() { bash todo-txt/todo.sh -p -d todo-txt/todo.cfg "$@" 2>&1 | sed -e 's/[0-9]\{4\}-[0-9]\{2\}-[0-9]\{2\}/DATE/g' -e "s|$PWD|PWD|g"; }
t add "Buy milk +shopping @town"
t add "Write report +work @office"
t a "Call mother @phone"
t addm "Fix bike +home
Read book +home @couch"
t ls
t ls +home
t ls -@phone
t pri 2 A
t p 3 b
t lsp
t lsprj
t lsc
t append 1 "and bread"
t prepend 4 "Today:"
t replace 5 "Read two books +home"
t depri 3
t ls
t do 1 2
t lsa
t archive
t ls
cat "$TODO_DIR/done.txt" | sed 's/[0-9]\{4\}-[0-9]\{2\}-[0-9]\{2\}/DATE/g'
t -f del 3 "Today:"
t -f rm 4
t add "Call mother @phone"
t add "Call mother @phone"
t deduplicate
t addto someday.txt "Learn the violin"
t lf someday.txt
t mv 1 someday.txt
t lf someday.txt
t ls
t do 99
t pri 1 1
t listaddons
t shorthelp | head -5
t -V | head -1
echo "files:"; ls "$TODO_DIR"
