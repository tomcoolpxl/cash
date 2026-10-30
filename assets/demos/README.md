# The README's clips

Each `.tape` here is a [VHS](https://github.com/charmbracelet/vhs) script, and the `.gif`
beside it is what it rendered. `scripts\render-demos.ps1` renders them all, or the ones
named on its command line, and shrinks them with gifsicle; `_settings.tape` holds what
they share (the Nerd Font build of Cascadia Mono, Windows Terminal's Campbell colours,
and the start of cash from cmd, hidden; each tape sets its own window size before
sourcing it), `setup.sh` makes the directory they run in
(`%PUBLIC%\cash-demo`, deleted again after a render), and `demo.bashrc` is the rc file
that cash starts with, so that a render touches neither the real `~/.bashrc` nor the
real history.

| clip | shows |
| --- | --- |
| paths | `cd C:\...` unquoted, `cd /c/...`, `pwd` printing `C:/...`, a `/c/` glob, Program Files with its space in both spellings, PowerShell given `$PWD`, an argument reaching cmd as typed, `shopt -s unixpaths` |
| script | a script with `find`, `xargs`, `grep`, `sed` and `awk` running, `type` showing them as builtins, `$BASH_VERSION` |
| prompt | a suggestion from history, a red unknown command, highlighting, the Ctrl-R picker |
| directories | back and forward through recent directories, `z`, Alt-E's picker to go to a directory and, after a command, to insert a path (Enter picks, Esc closes; Ctrl-Enter, which does both, reaches cash as a plain Enter from VHS); the keys are named in typed comment lines |
| jobs | Ctrl-Z, `jobs`, `bg`, `fg`, Ctrl-C giving 130, `kill %1` |
| ports | `ss -ltn`, `lsof -i :8080`, `kill $(lsof -t ...)`, a file `rm` cannot delete, `fuser -v`, `fuser -k` |
| top | `top`, a meter for each core (`1`, two to a row), sorted by memory, then as a tree |
| archivers | `tar`, `zip`, `7z` and `rar` of the same directory, `unrar l`, `type` |
| help | `figlet`, the install hint for a missing command, `help -d`, `man sed` |
| sextant | the game playing itself (`--demo`), drawn with the font's own sextant glyphs (`--unicode`) |

Two things to know before changing a tape:

- VHS starts a shell from a fixed list, so every tape starts cmd and types `cash
  --rcfile demo.bashrc` into it while hidden. VHS draws in xterm.js, not Windows
  Terminal: no sixel, no soft fonts, and Nerd Font icons only with `Set FontFamily`.
  So the sextant clip runs the game with `--unicode`, the font's own sextant glyphs,
  which is why the clips' font is the Nerd Font build of Cascadia Mono: the Cascadia
  that Windows ships is older than the 2404.23 release that added those glyphs. A
  `Set FontFamily` after the `Source` line comes too late; the terminal is open by then.
- VHS's `Alt+` takes one character, so Alt-Left and Alt-Right cannot be pressed. The
  directories tape presses Ctrl-O and Ctrl-], which `demo.bashrc` binds to `prevd` and
  `nextd`, the builtins the arrows run.

## Rendering

```
scoop install vhs ttyd ffmpeg gifsicle extras/chromium
scoop bucket add nerd-fonts
scoop install nerd-fonts/CascadiaMono-NF
powershell -File scripts\render-demos.ps1
```

VHS 0.12 launches the first `chrome` on PATH, else Edge, and takes Edge's start for a
crash, because Edge's first process hands off to a second one; the script puts Scoop's
chromium first on its own PATH. A clip is 1000 by 560 pixels at 15 frames a second,
shown at half width in the README; keep one under 2 MB, which gifsicle's `--lossy=40
--colors 128` gives for 15 seconds of terminal. The game is the exception, near 3 MB:
its whole screen changes every frame, so its tape runs at 12 frames a second.
