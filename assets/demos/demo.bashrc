# The rc file the README's demo clips start cash with (`cash --rcfile`, from the tapes
# beside it). It replaces ~/.bashrc for that cash: a short prompt, the demo directory
# that setup.sh makes, and a history of its own, so that a render leaves nothing in the
# real history. `scripts\render-demos.ps1` runs setup.sh first.

demo="$PUBLIC/cash-demo"
[[ -d $demo ]] || { echo "demo.bashrc: run assets/demos/setup.sh first" >&2; return; }

PS1='\[\e[1;34m\]\W\[\e[0m\] \$ '
HISTFILE="$demo/.history"
cd "$demo/project"

# The directories clip. VHS cannot press Alt-Left or Alt-Right (its Alt takes one
# character), so the clip presses Ctrl-O and Ctrl-], bound here to the builtins the
# arrows run; the viewer sees the same thing happen.
bind -x '"\C-o": prevd'
bind -x '"\C-]": nextd'
