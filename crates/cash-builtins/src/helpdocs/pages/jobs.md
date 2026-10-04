---
names: jobs fg bg
see: kill wait disown detach job-control
spec: D6 D11 D13 D19 D70
---
## Description

`jobs` lists the shell's jobs, `-l` with their process ids, `-p` with the ids alone.
`fg [%N]` brings a job to the foreground, `bg [%N]` resumes a stopped one in the
background. As in Bash, `fg` and `bg` need job control: in a script they say
`no job control` until `set -m`.

## Windows notes

- Each job runs in a Windows job object of its own, so killing it ends its whole tree
  (D6). See `help job-control` for Ctrl-C, Ctrl-Z and `kill`.
- Ctrl-Z stops a foreground job by suspending its threads (D19); `fg` and `bg` resume
  it. A full-screen program is asked to redraw when it comes back.
- Under `fg`, Ctrl-C reaches a job started with `&` as a Ctrl-Break, and a second
  Ctrl-C ends its tree (D13).
- A background job that runs shell code (`{ ...; } &`, a loop) runs on a thread of
  cash's own process; `jobs -p` shows a number of cash's own for it (D70).
