---
names: jobs fg bg
see: kill wait disown detach job-control
---
## Description

`jobs` lists the shell's jobs, `-l` with their process ids, `-p` with the ids alone.
`fg [%N]` brings a job to the foreground, `bg [%N]` resumes a stopped one in the
background. As in Bash, `fg` and `bg` need job control: in a script they say
`no job control` until `set -m`.

## Windows notes

- Each job runs in a Windows job object of its own, so killing it ends its whole tree. See `help job-control` for Ctrl-C, Ctrl-Z and `kill`.
- Ctrl-Z stops a foreground job by suspending its threads; `fg` and `bg` resume
  it. A full-screen program is asked to redraw when it comes back.
- Under `fg`, Ctrl-C reaches a job started with `&` as a Ctrl-Break, and a second
  Ctrl-C ends its tree.
- A background job that runs shell code (`{ ...; } &`, a loop) runs on a thread of
  cash's own process; `jobs -p` shows a number of cash's own for it.
