# Trying `sudo`, `su` and `sudoedit` by hand

The tests stop where Windows asks a person: a UAC prompt, a password, a second account.
These are the checks that need one. Run them in a cash in Windows Terminal, unelevated,
with gsudo installed unless a step says otherwise. `sudo -l` says which tool elevates.

## Elevation

- [ ] `sudo whoami /groups | grep -i high` approves once at UAC and prints the High
      Mandatory Level line, in this terminal.
- [ ] `sudo -v`, then `sudo true` twice: one prompt for the three. `sudo -l` says the cache
      is open; `sudo -k` closes it, and `sudo -n true` then fails at once with
      `sudo: a password is required` (status 1).
- [ ] `sudo cmd /c "exit 7"; echo $?` prints 7. `sudo false; echo $?` prints 1.
- [ ] `sudo printf '%s\n' 'a & b' 'c | d' '%PATH%'` prints the three words as typed:
      nothing is run by a command processor.
- [ ] `echo x | sudo tee C:/Windows/Temp/try-sudo.txt` writes `x` there, and
      `sudo cat C:/Windows/Temp/try-sudo.txt | wc -c` counts 2: pipes reach in and out.
- [ ] `sudo ping -t 127.0.0.1`, then Ctrl-C: the ping stops, cash stays, the prompt comes
      back.
- [ ] `sudo -i` starts an elevated cash in your home folder as a login shell; `sudo -s` one
      in this folder. `exit` returns.
- [ ] `export FOO=1; sudo -E printenv FOO` prints 1, with gsudo and with Windows' sudo.

## Who owns what an elevated command makes

- [ ] `sudo touch C:/Windows/Temp/try-owner.txt; ls -l C:/Windows/Temp/try-owner.txt`
      shows you as the owner, not Administrators. Remove it with `sudo rm`.
- [ ] Signed in as a standard account, `sudo whoami` asks for an administrator's password at
      UAC, prints `sudo: running as PC\admin, not you; files and ~ are theirs` once, then the
      administrator's name.

## Windows' sudo and UAC (uninstall or rename gsudo for these)

- [ ] Windows' sudo in new-window mode: `sudo cmd /c cd` opens a window showing this folder,
      not `C:\Windows\System32`, and cash says the status is not the command's.
- [ ] `sudo config --enable normal` in an elevated shell; then `sudo whoami` runs here.
- [ ] Windows' sudo turned off: `sudo whoami` asks UAC, opens a new window, and cash says it
      is not waited for.

## A mapped network drive

- [ ] `net use Z: \\server\share`, `cd Z:/`, then `sudo cmd /c cd`: with gsudo the folder is
      `Z:\` (gsudo maps it, `--copyns`); with Windows' sudo cash says it uses
      `\\server\share` and the command shows that folder.

## Another account

- [ ] `su alice` and `sudo -u alice whoami` with cash under `~/scoop`: refused at once,
      naming cash.exe and `scoop install -g cash`, before any password.
- [ ] After `scoop install -g cash` (or with cash under Program Files), `sudo -u alice
      whoami` asks alice's password and prints `alice`; `su alice` gives a cash as alice,
      `su - alice` one in her home folder.
- [ ] Without gsudo, `su alice` says runas opens a new window, and runas asks the password
      there.

## sudoedit

- [ ] `sudoedit C:/Windows/System32/drivers/etc/hosts` opens a copy named `hosts.sudoedit-…`
      in Notepad (unelevated: its title bar has no Administrator). Add a comment line, save,
      close: one UAC prompt, and `cat` shows the line in `hosts`. `icacls` on `hosts` shows
      the same access list as before.
- [ ] The same, closing without a change: `sudoedit: …/hosts unchanged`, and no prompt.
- [ ] `EDITOR='code --wait' sudoedit C:/ProgramData/try-sudoedit.txt` creates the file
      when saved, owned by you.
- [ ] Notepad as the editor: check that cash waits for the window to close. If it does not
      (Windows 11's Notepad hands the file to the Store app and returns), set `EDITOR` to an
      editor that waits.
- [ ] No copies are left in `$TEMP` (`ls $TEMP/*.sudoedit-*`).

## Tab

- [ ] `su <Tab>` and `sudo -u <Tab>` list the local accounts (not disabled ones such as
      Guest); `sudo -u alice ec<Tab>` completes `echo`.
