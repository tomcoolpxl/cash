# source: https://github.com/magicmonty/bash-git-prompt/tree/4501746 (`gitprompt.sh`, `gitstatus.sh`, `prompt-colors.sh`, `git-prompt-help.sh`, `themes` and LICENSE.txt, unchanged, as the fixture directory `bash-git-prompt`)
# desc: bash-git-prompt, the informative Git prompt: gitstatus.sh's eleven lines on a clone that is clean, then with staged, changed and untracked files, a stash, a commit ahead of its upstream and a detached head; gitprompt.sh sourced, its themes listed, the prompt built and printed for each state and after three more themes are loaded, the exit-status and path formatting, and its help
# timeout: 120
export HOME="$PWD/home"; mkdir -p "$HOME"
# Git's global settings are the case's own, under its HOME: a machine's core.autocrlf
# made the stash pop leave files to stage here and not there.
git config --global core.autocrlf false
export GIT_AUTHOR_NAME=A GIT_AUTHOR_EMAIL=a@example.com GIT_COMMITTER_NAME=A GIT_COMMITTER_EMAIL=a@example.com
export GIT_AUTHOR_DATE='2026-01-02T03:04:05Z' GIT_COMMITTER_DATE='2026-01-02T03:04:05Z'
export GIT_PROMPT_FETCH_REMOTE_STATUS=0 GIT_PROMPT_ONLY_IN_REPO=0 GIT_PROMPT_THEME=Default
gp=$PWD/bash-git-prompt
git init -q -b main origin && (cd origin && echo a > tracked && git add tracked && git commit -q -m one) || exit 1
git clone -q origin repo 2> /dev/null && cd repo || exit 1
status() { bash "$gp/gitstatus.sh" | tr '\n' '|' | sed 's/_PREHASH_[0-9a-f]*/_PREHASH_HASH/'; echo; }
# An elevated shell is root to cash ($EUID is 0 there), and a CI runner runs elevated,
# so the prompt ends in the root's `#` there; the end is shown as the user's.
prompt() { printf '%s\n' "$PS1" | sed 's/ # $/ $ /'; }
echo "== clean"; status
echo b >> tracked; echo c > staged; git add staged; echo d > untracked
echo "== staged, changed, untracked"; status
git stash -q; echo "== stashed, the untracked file kept"; status; git stash pop -q
git commit -q -am two; echo "== ahead of origin"; status
git checkout -q --detach HEAD~1; echo "== detached"; status; git checkout -q main
echo "== sourced"
. "$gp/gitprompt.sh"
echo "prompt command: ${PROMPT_COMMAND%%;*}"
echo "== themes"; git_prompt_list_themes | sed -n '1,8p'
echo "== prompt, Default"; setGitPrompt; prompt
rm untracked; echo e >> tracked; git add -A; echo "== prompt, staged only"; setGitPrompt; prompt
git commit -q -m three; echo "== prompt, clean and ahead"; setGitPrompt; prompt
for theme in Single_line Solarized Minimal; do
  echo "== prompt, $theme"; GIT_PROMPT_THEME=$theme; git_prompt_load_theme; setGitPrompt; prompt
done
echo "== exit status"; for s in 0 1 2 126 127 130 143; do printf '%s: %s\n' "$s" "$(gp_format_exit_status "$s")"; done
echo "== truncated path"; GIT_PROMPT_MAX_PWD=16; cd "$HOME" || exit 1; mkdir -p one/two/three/four/five; cd one/two/three/four/five || exit 1
gp_truncate_pwd | sed "s|${HOME//\\//}|HOME|"
echo "== outside a repository"; GIT_PROMPT_ONLY_IN_REPO=1; setGitPrompt; prompt
echo "== help"; type git_prompt_help > /dev/null 2>&1 && git_prompt_help | sed -n '1,6p'
