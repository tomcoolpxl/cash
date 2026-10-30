# Windows Terminal

`cash --terminal-profile`, which Scoop and the installer run on install, adds a `cash`
profile with the logo and command marks on: each command gets a mark on the scrollbar,
green or red by its exit status once the prompt reports it (below). Its GUID is
`{43e4cdd3-eb67-5e13-bd17-fa0d7f8cf3ff}`. To make it the default, or give it a font and
colours, add an entry with that GUID to Windows Terminal's `settings.json` (Settings,
Open JSON file) holding only your own settings; the program and icon keep coming from the
install, so upgrades need nothing. If your `newTabMenu` lists profiles one by one, the
install adds cash to the end of it, and uninstalling takes it out; nothing else in the
file changes. Ctrl+↑ and Ctrl+↓ jump between marks with the two actions below.

The profile asks for a Nerd Font when the machine has one, so that `ls --icons` draws
its file icons: a Cascadia one, else the Nerd Font your other Terminal profiles use, else
any monospaced one. With none, `scoop install nerd-fonts/CascadiaMono-NF`, then `cash
--terminal-profile` again, gives it one.

```jsonc
"defaultProfile": "{43e4cdd3-eb67-5e13-bd17-fa0d7f8cf3ff}",
"profiles": { "list": [ {
    "guid": "{43e4cdd3-eb67-5e13-bd17-fa0d7f8cf3ff}",
    "source": "cash",
    "startingDirectory": "%USERPROFILE%",
    "font": { "face": "UbuntuSansMono Nerd Font" }
} ] },
"actions": [
    { "command": { "action": "scrollToMark", "direction": "previous" }, "id": "User.scrollToMark.previous" },
    { "command": { "action": "scrollToMark", "direction": "next" }, "id": "User.scrollToMark.next" }
],
"keybindings": [
    { "id": "User.scrollToMark.previous", "keys": "ctrl+up" },
    { "id": "User.scrollToMark.next", "keys": "ctrl+down" }
]
```

`cash --remove-terminal-profile` undoes the profile. Alt-arrows reach cash only in a tab
that is not split, since Windows Terminal uses them for its panes.

## The prompt sequences

A build of your own, outside an install, needs a whole profile instead: any GUID, the
same in `defaultProfile`, a `name`, and `commandline` set to that `cash.exe`. The marks'
colours come from the prompt, which has to send Microsoft's
[shell-integration sequences](https://learn.microsoft.com/windows/terminal/tutorials/shell-integration):
`OSC 133;A` and `B` around `PS1`, `C` in `PS0`, `D;$?` when a command ends, and `OSC 9;9`
with the directory in Windows form so that a new tab or split pane opens where you are.
The starter `~/.bashrc` the install writes does this; in one of your own, set from
`PROMPT_COMMAND` so the exit status is the last command's:

```bash
__wt_prompt() {
    local status=$? cwd=${PWD//\//\\}
    PS1="\[\e]133;D;$status\e\\\\\]\[\e]133;A\e\\\\\]\[\e]9;9;${cwd//\\/\\\\}\e\\\\\]\w \$ \[\e]133;B\e\\\\\]"
}
[[ -n $WT_SESSION ]] && { PROMPT_COMMAND=__wt_prompt; PS0='\[\e]133;C\e\\\]'; }
```

The directory's backslashes are doubled because in `PS1` a lone backslash starts an escape
(`\t` would print the time). Git Bash, whose `$PWD` is `/c/...`, needs it turned into
`C:\...` first.
