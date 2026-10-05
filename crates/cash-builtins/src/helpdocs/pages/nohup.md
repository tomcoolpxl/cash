---
see: detach disown job-control
---
## Description

`nohup COMMAND` runs a command with GNU `nohup`'s streams: input from a terminal is
replaced by an empty one, output to a terminal goes to `nohup.out` in the current folder
(or the home folder), and errors to a terminal follow the output.

## Windows notes

On Windows a hangup is the console closing, and what cash started ends with it, so
`nohup cmd &` does not keep a program running after you close the window. Start such a
program with `detach` instead.
