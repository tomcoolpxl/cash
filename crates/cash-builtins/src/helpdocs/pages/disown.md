---
see: detach jobs nohup job-control
---
## Description

`disown [%N]` removes a job from the job table: `jobs` no longer lists it, `wait` no
longer waits for it, and `kill %N` no longer finds it. `-a` disowns every job.

## Windows notes

On Linux a disowned job also outlives the shell. On Windows every program cash starts is
in cash's job object, which no process can leave once in it, so a disowned job still ends
when cash is closed. To start a program that outlives cash, use `detach`.
