# `sudo` in this terminal, without gsudo

Asked for by the user on 2026-10-10 as a candidate (TODO.md), and built the same day
after the small items of phase 35 (DONE.md). The choices in section 6 are the user's,
by pick list; section 7 is what the probes of section 4 showed. Sections 1 and 2 are as
things stood before it was built.

## 1. What cash did before

`sudo` is cash's builtin, but cash elevates nothing itself. `route()` in
`crates/cash-builtins/src/win.rs` hands the command to:

1. **gsudo** where it is on `PATH`: in this terminal, waited for, its status returned;
2. else **Windows' own `sudo`** (Windows 11 24H2 and later, once turned on), in the mode
   it is set to: inline and input-closed keep the command here, `forceNewWindow` (its
   default) opens a new window and does not wait;
3. else **UAC's request**, as `elevate` makes it: a new window, not waited for. The
   status says only that the command started, and its output stays in that window.

Whatever elevates, the command runs under `cash --invoke-bundled --sudo-owner SID`
(`cash_win32::account::run_owned_by`), which makes the user who asked the owner of what
it creates. On the user's machine gsudo 2.6.1 is installed, and Windows' sudo is on in
its new-window mode, so `sudo` elevates in the terminal through gsudo; on a machine
without gsudo it opens a window.

## 2. How the others do it

**gsudo** (MIT, gerardog.github.io/gsudo) starts an elevated copy of itself with the
`RunAs` verb, which serves one elevation (or several, with its cache on) over two named
pipes in the `ProtectedPrefix\Administrators` namespace, one for data and one for
control. Four modes:

- *TokenSwitch* (the default): the unelevated gsudo creates the command suspended,
  through undocumented `kernel32` calls; the elevated one replaces its primary token and
  resumes it. Redirection and the console are native; the command keeps the caller's
  environment. Elevating as another user falls back to Attached or Piped.
- *Attached*: the elevated gsudo attaches to the caller's console with `AttachConsole`;
  Windows and the console host do the rest. Native console, no redirection; the elevated
  account's environment.
- *VT*: the command runs in a pseudo console of the elevated gsudo's, relayed over the
  pipes. No resizing.
- *Piped*: input and output relayed as bytes; no console, so no Tab completion and
  trouble with code pages.

Its cache is off by default: it lets the processes of the one that opened it elevate
without asking, so a process that gets code into one of them elevates in silence.

**Windows' sudo** (MIT, github.com/microsoft/sudo) connects an unelevated `sudo.exe` to
an elevated one over RPC. `normal` (inline) runs the command in the caller's console
with its input, `disableInput` closes the input, and `forceNewWindow` is the default.
Microsoft's page warns that in the first two "an unelevated process can send input to
the elevated process within the same console window or get information from the
output".

## 3. What cash would do

The proposal is gsudo's Attached mode, with the one thing it lacks, redirection, taken
another way, and everything else as cash's `sudo` does it now.

1. **Elevation.** cash starts itself, `cash.exe --invoke-bundled --sudo-attach ...`,
   through `ShellExecuteExW` with the `runas` verb and its window hidden
   (`SW_HIDE`), and waits on the process handle that `SEE_MASK_NOCLOSEPROCESS` gives
   back. The command's status is the elevated cash's exit code
   (`GetExitCodeProcess`). UAC asks as it asks for any program; nothing else is
   installed or left running.
2. **The console.** The elevated cash leaves the console Windows gave it
   (`FreeConsole`) and attaches to the caller's (`AttachConsole(PID)`): the window of a
   console host, or the pseudo console of Windows Terminal and the other terminals.
   The command then has the terminal as any program cash starts has it: colours, cursor,
   `vim`, a password prompt.
3. **Redirections and pipes.** Where a standard handle of the `sudo` command is not the
   console (`sudo cat f | grep x`, `sudo tee /etc/x < y`, `sudo ls > list`), the
   elevated cash takes that handle from the caller with `DuplicateHandle`: an
   administrator's process may open the user's own with `PROCESS_DUP_HANDLE`. The
   caller passes the handle values. Nothing is relayed, so nothing is slower and no
   byte is translated. (gsudo's Attached mode has no redirection, and its Piped mode
   relays.)
4. **Environment and folder.** The caller's folder (a mapped drive's by its UNC path,
   as now: an elevated process has no drive letters) and the variables cash's `sudo`
   passes now: `NAME=value` words, and the exported ones with `-E`. They go through a
   handle as well (an anonymous pipe the caller writes and the elevated cash reads), so
   Windows' 32,000-character command line no longer limits `-E`.
5. **The command** runs under `run_owned_by`, as now, in a job of the elevated cash's
   that ends with it. cash's own session job (spec D6, D42) cannot take an elevated
   process, so the elevated cash also waits on the caller and ends its job if the
   caller goes first: a killed cash leaves nothing elevated behind.
6. **Ctrl-C.** The console sends its control events to every process attached to it,
   so the elevated command gets the Ctrl-C a user types, as Unix's does through the
   terminal; the elevated cash ignores it, as `run_owned_by` does now, and returns the
   command's status. cash's wait treats it as any foreground program's.

Not changed: `sudo -u USER` and `su USER` (another account's password: gsudo, else
`runas` in a new window), `sudoedit`'s copies, the checks before a password is asked,
`sudo -l` (which would name the new route), `cash doctor`.

## 4. To be seen first

Each needs a UAC approval, so the user clicks while it runs.

- **Attaching across the levels**: whether an elevated process may `AttachConsole` to
  an unelevated cash's console on Windows 11 (build 26300 here), in Windows Terminal
  and in a console window. gsudo's Attached mode says it may; cash's own is to be seen.
- **Ctrl-C**: whether the elevated command gets it (`sudo ping -t 127.0.0.1`), and
  Ctrl-Break, and closing the window.
- **Handles**: whether `DuplicateHandle` from the caller gives the elevated command a
  pipe and a file that work as its own, both ways.
- **The window**: whether the elevated cash's own console shows for a moment before
  `FreeConsole`, with `SW_HIDE`.
- **The time**: from Enter to the command's first output, less the time UAC waits for
  the click.
- **A standard account**: UAC then asks for an administrator's password; the command
  runs as that administrator, and `run_owned_by` already says so.

## 5. Security

In this terminal, an unelevated program running beside the elevated command on the same
console can type into it and read what it prints. That is so for gsudo's default and
Windows' inline mode alike; Microsoft's default is a new window for that reason, and
both offer to close the input. UAC on the same desktop is a convenience, not a boundary,
as Microsoft says of it and gsudo's page repeats.

A credentials cache is not proposed. It is an elevated process that grants elevation
without asking, and any process of the user's could try to be the one it grants it to.
`sudo -v`, `-k` and `-K` keep meaning gsudo's cache where gsudo is installed.

## 6. The choices

Made by the user on 2026-10-10, by pick list:

1. **Which comes first: cash's own, always.** One behaviour on every machine, no other
   tool needed; gsudo only for `-u USER` and its cache (`-v`, `-k`, `-K`). Windows' sudo
   and UAC's new window are no longer used by `sudo`. (Not chosen: gsudo first, then
   cash's own; cash's own only where neither gsudo nor Windows' sudo is.)
2. **Input: the command reads the terminal**, as Unix's `sudo`, gsudo's default and
   Windows' inline mode do. (Not chosen: the input closed; a way to close it.)
3. **The probes of section 4: at once**, with the user at the UAC prompts.

## 7. What the probes showed

2026-10-10, Windows 11 build 26300, in Claude's terminal panel (a pseudo console), with
the mechanism of section 3 built into a debug cash (`cash_win32::elevate`):

- **Elevation and the status**: UAC asks for cash.exe; approved, the command runs
  elevated and its status comes back (`sudo cmd /c 'cd & exit 7'` gives 7). Declined, or
  left until UAC gives up, `sudo` says so, `sudo: not run: the request to run it as an
  administrator was declined`, with 1.
- **The console**: the elevated cash attaches to the caller's pseudo console; output
  and errors show in it.
- **The folder**: the command starts in the shell's folder.
- **Handles**: a file given as standard input and one as standard output
  (`sudo sort < in > out`) reach the command through `DuplicateHandle`; nothing is
  relayed.
- **Ctrl-C**: typed at the console, it reaches the elevated `ping.exe`, which ends; the
  script then ends with 130, as for any program Ctrl-C ends
  (`sudo_in_terminal::ctrl_c_reaches_the_elevated_command`, run by hand).
- **Keys**: a line typed at the console reaches an elevated `read`
  (`sudo bash -c 'read -r -p "name? " x; ...'`), and the status comes back
  (`sudo_in_terminal::the_elevated_command_reads_the_keys_typed`, run by hand).
- **UAC's prompt** shows at once when the terminal is in front; when the asking program
  is not, Windows shows it only as a blinking shield in the taskbar, and a prompt left
  there long enough counts as declined.

Not built yet: the environment through a handle (section 3, item 4). `-E` still passes
the exported variables as `NAME=value` words, within Windows' 32,000 characters.
