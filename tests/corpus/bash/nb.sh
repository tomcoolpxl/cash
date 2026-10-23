# source: https://github.com/xwmx/nb/tree/7.25.5 (`nb`, unchanged, with its LICENSE, as the fixture directory `nb`)
# desc: a notes, bookmarks and todos program of one 27,000-line bash script over git, run offline: its first start, notes added and listed, shown, searched, a second notebook added and filled, a todo added, listed, done and deleted, an item moved, notebooks listed, its version; colour off, a dumb terminal 80 wide, the timestamps of its file names left out
# timeout: 180
export HOME="$PWD/home" NB_DIR="$PWD/nb-data" NB_AUTO_SYNC=0 EDITOR=true
export _COLOR_ENABLED=0 TERM=dumb COLUMNS=80
mkdir -p "$HOME"
git config --global user.name u
git config --global user.email u@example.com
git config --global init.defaultBranch main
n() {
    bash nb/nb "$@" 2>&1 | sed -E -e 's/[0-9]{14}/STAMP/g' -e "s|$PWD|PWD|g"
    echo "[${PIPESTATUS[0]}]"
}
n
n add "The first note body" --title "First note"
n add "Second body, with #tag1 and #tag2" --title "Second"
n list
n show 1 --print
n show 2 --info-line
n search body
n search "#tag2"
n notebooks add work
n work:add "work item" --title "Work"
n work:list
n notebooks
n todo add "Buy milk"
n todos
n do 3
n todos
n delete 3 --force
n move 1 work: --force
n list
n work:list
n --version
