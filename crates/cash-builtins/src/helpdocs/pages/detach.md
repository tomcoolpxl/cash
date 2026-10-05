---
see: disown nohup start job-control
---
## Description

`detach COMMAND` starts a command that outlives the shell: outside cash's job object,
with no console, and not waited for. It prints the new process's id and returns.

## Windows notes

Everything else cash starts lives in its job object and ends when cash does; `&`,
`disown` and `nohup` do not change that. `detach` is the one way out. The command runs
in the shell's folder with its exported variables, and is found as the shell would find
it. It gets none of the shell's streams, so its output is lost; a redirection on the
`detach` line applies to `detach`'s own message. To keep the output, detach a cash that
redirects it, as in the second example.

Allowing it has a cost, accepted: for one program to leave the session's job, the job
must let any program ask to leave it.

## Examples

```
detach code .
detach cash -c './server.exe --port 8080 > server.log 2>&1'
```
