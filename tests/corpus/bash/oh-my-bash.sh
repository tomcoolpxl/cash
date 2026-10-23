# source: https://github.com/ohmybash/oh-my-bash/tree/4d86087 (`oh-my-bash.sh`, `lib`, `tools/git-prompt.sh`, the themes font, minimal, agnoster, powerline and powerline-plain, the plugins git and bashmarks, and four alias files, unchanged, with its LICENSE.md, as the fixture directory `oh-my-bash`)
# desc: a prompt framework loaded into interactive shells with no terminal: four themes' prompts built by their prompt commands and shown with cat -v, outside and inside a fresh git repository; its aliases, its git plugin and bashmarks' save, list and go; the clock left out
# timeout: 120
export HOME="$PWD/home" OSH="$PWD/oh-my-bash" DISABLE_AUTO_UPDATE=true THEME_SHOW_CLOCK=false
mkdir -p "$HOME"
omb() {
    OSH_THEME="$1" bash --norc --noprofile -i -c '
        plugins=(git bashmarks)
        aliases=(general ls misc chmod)
        completions=()
        source "$OSH/oh-my-bash.sh"
        echo "loaded ${OSH_THEME:-no theme}: ${OMB_VERSION}"
        '"$2" 2>/dev/null < /dev/null | sed -e "s|$PWD|PWD|g" -e "s|${USER:-$USERNAME}@|USER@|g"
}
prompt='for c in "${PROMPT_COMMAND[@]}"; do eval "$c"; done; printf "%s\n" "$PS1" | cat -v'
for theme in font minimal agnoster powerline-plain; do
    omb "$theme" "$prompt"
done
git -c init.defaultBranch=main init -q repo
(
    cd repo || exit
    git -c user.name=u -c user.email=u@example.com commit -q --allow-empty -m first
    omb font "$prompt"
    echo change > file.txt
    omb font "$prompt"
    omb powerline-plain "$prompt"
)
omb "" '
for name in ll la l .. ... gst ga gcmsg glog; do alias "$name" 2>&1; done
type -t gst _omb_util_command_exists _omb_prompt_git
_omb_util_command_exists git && echo "git is a command"
_omb_util_command_exists no-such-command || echo "no-such-command is not"
cd repo && s here && cd .. && g here && pwd
cat "$HOME/.sdirs"
'
