# source: https://github.com/fidian/ansi/blob/master/ansi (the fixture `ansi`, unchanged)
# desc: a real option-heavy script sourced: getopts-free option parsing in a loop, colour tables, printf of escapes; ANSI_FORCE_SUPPORT keeps tput's answer out of it
export ANSI_FORCE_SUPPORT=1
. ./ansi
show() { "$@" | od -An -c | tr -s ' ' | sed 's/^ //'; }
show ansi --bold --red "bold red"
show ansi --underline --bg-blue --white "under"
show ansi --color=31 --bg-color=44 text
show ansi -n --green "no reset"
show ansi --no-restore --italic plain
show ansi --rgb=255,128,0 rgb
show ansi --bg-rgb=1,2,3 bgrgb
show ansi --black --bg-white-intense intense
show ansi --up=2 --down --forward=3 --backward=1
show ansi --reset-attrib
show ansi --title="a title"
ansi --unknown-option text 2>&1 >/dev/null | head -1
echo "status $?"
