---
names: cashctl cashinfo
see: job-control config
spec: D6
---
## Description

`cashctl` (also `cashinfo`) inspects and changes the running shell.

- `cashctl gui-apps`: whether GUI programs started from cash (`code .`) keep running
  after cash exits. `outlive`, the default, keeps them, as PowerShell does; `close`
  closes them with cash, for this session (D6).
- `cashctl events status`, `enable EVENT`, `disable EVENT`: cash's debug logging, as
  `--debug EVENT` sets it at startup.
- `cashctl call stack`, `cashctl complete line LINE`, `cashctl process pid`: what the
  shell is doing, for debugging.

The settings last for the session; put a `cashctl` line in `~/.cashrc` to keep one.
