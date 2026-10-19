---
names: cashctl cashinfo
summary: Inspect and configure the running shell.
summary cashinfo: Inspect and configure the running shell (the same as `cashctl`).
kind: cash's shell commands
see: job-control config
---

## Syntax

    cashctl COMMAND [ARG]...

    cashctl gui-apps            whether GUI programs outlive cash
    cashctl gui-apps close      close them with cash, this session
    cashctl events status       the debug events that are on
    cashctl call stack          where the running script is

## Examples

The setting for GUI programs, and the call stack of a function:

    cashctl gui-apps
    f() { cashctl call stack; }
    f > /dev/null && echo "inspected"

## How it works

`cashctl` (also `cashinfo`) inspects and changes the running shell.

- `cashctl gui-apps`: whether GUI programs started from cash (`code .`) keep running
  after cash exits. `outlive`, the default, keeps them, as PowerShell does; `close`
  closes them with cash, for this session.
- `cashctl events status`, `enable EVENT`, `disable EVENT`: cash's debug logging, as
  `--debug EVENT` sets it at startup.
- `cashctl call stack`, `cashctl complete line LINE`, `cashctl process pid`: what the
  shell is doing, for debugging.

The settings last for the session; put a `cashctl` line in `~/.cashrc` to keep one.
## Options

    gui-apps [outlive|close]     GUI programs outlive cash, or close.
    events status                The debug events that are on.
    events enable EVENT          Turn one on, as --debug EVENT does.
    events disable EVENT         Turn it off.
    call stack                   The functions and files running now.
    complete line LINE           The completions Tab offers for LINE.
    process pid                  This shell's process id.

Exit status: 0, or 2 for a command it does not know.

