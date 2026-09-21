Let's say I want to run Bash on Windows. Let's say I don't care about PowerShell stuff. So I want actually to have a native Bash on Windows, and I know that the paths are a problem, so I want a good solution for that, and it must run natively. So I know there's a number of options, right, from WSL, which is not really native, that's just a VM, to Git Bash and other Bashes. But none of them integrate nicely. Is it actually possible to build a native integrated Bash for Windows?

Yes. A genuinely native Windows Bash is technically possible, but it would be a substantial port of Bash rather than simply recompiling GNU Bash with MinGW.

There is even historical proof: `win-bash` was a port of GNU Bash that did not depend on Cygwin or another POSIX runtime. It was eventually abandoned because the code became old and difficult to maintain, not because the idea was fundamentally impossible. ([SourceForge][1])

The important distinction is this:

| Approach       | Linux kernel | POSIX emulation DLL |   Native Windows processes |         Bash compatibility |
| -------------- | -----------: | ------------------: | -------------------------: | -------------------------: |
| WSL2           |      Yes, VM |                  No |                    Interop |                  Excellent |
| WSL1           |        No VM | Syscall translation |                    Interop |                  Excellent |
| Cygwin         |           No |       `cygwin1.dll` | Yes, but POSIX personality |                  Excellent |
| MSYS2/Git Bash |           No |      `msys-2.0.dll` |                        Yes |                  Very good |
| "Win32 Bash"   |           No |                  No |                        Yes | Would require a major port |

One correction to the premise: WSL2 is indeed built around a lightweight VM running a real Linux kernel. WSL1 is different; it does not use that architecture. Microsoft still documents both, although WSL2 is the default. ([Microsoft Learn][2])

## The path problem is actually solvable

I would not reproduce MSYS2's path translation model.

MSYS2 has two filesystem namespaces and automatically rewrites arguments such as:

```text
/foo
```

into Windows paths when invoking native programs. It even has escape mechanisms such as `MSYS2_ARG_CONV_EXCL` because the heuristic sometimes gets things wrong. ([MSYS2][3])

A Windows-native Bash could instead make Windows paths first-class:

```bash
cd C:/Users/me/projects/foo

gcc C:/Users/me/projects/foo/main.c

export SDK=C:/SDKs/foo
```

That syntax already fits Bash's lexer nicely. The colon in `C:` is not a problem, and `/` avoids Bash interpreting Windows backslashes as escape characters.

Internally you would use UTF-16 Win32 paths:

```text
C:/Users/me/foo
        ↓
C:\Users\me\foo
        ↓
CreateFileW(...)
```

and never translate arguments passed to another native program unless there is a very specific reason.

You could additionally provide Unix-style aliases as a convenience:

```text
/c/Users/me      -> C:\Users\me
/d/data          -> D:\data
/tmp             -> %TEMP%
/dev/null        -> NUL
```

But those should be a compatibility feature, not the underlying filesystem model.

Windows itself has a much richer path namespace than the normal `C:\foo` representation, including UNC and `\\?\` paths, so the internal path layer could support those properly as well. ([Microsoft Learn][4])

## The actual nightmare is `fork()`

This is where a straightforward port stops working.

Git for Windows explicitly points this out: Bash expects `fork()`, while Win32 fundamentally uses `CreateProcess()`. MSYS2/Cygwin go to considerable lengths to emulate `fork()`. ([gitforwindows.org][5])

For example:

```bash
foo=hello

(
    foo=world
    something
)

echo "$foo"
```

A Unix Bash effectively gets a copy of the shell process for the subshell.

Likewise:

```bash
generate | while read x; do
    ...
done
```

and:

```bash
result=$(some complicated shell expression)
```

and:

```bash
function x() {
    local foo=123
    ...
}

x | other-command
```

can require separate shell execution contexts.

Windows cannot simply say:

```c
pid = fork();
```

So your native Bash would need to change Bash's execution engine.

Instead of general-purpose `fork()`, I would implement something conceptually like:

```text
bash.exe
   |
   +-- parse/expand normally
   |
   +-- external executable?
   |       CreateProcessW(...)
   |
   +-- shell subshell required?
           start another bash.exe
           +
           transfer shell state
           +
           transfer AST/command
           +
           transfer inheritable handles
```

That second Bash process would receive a serialized shell state containing things such as:

```text
variables
functions
aliases
shell options
positional parameters
working directory
redirections
trap configuration
environment
AST to execute
```

possibly through shared memory rather than command-line arguments.

That is quite a project, but it avoids implementing a general POSIX `fork()` emulation layer.

## Native Windows programs would actually work better

This is where such a project could be substantially nicer than Git Bash.

Suppose you type:

```bash
clang main.c -o build/foo.exe
./build/foo.exe
```

The shell would eventually do essentially:

```c
CreateProcessW(...)
```

and hand it Windows-native handles, environment variables and paths.

There would be no:

```text
MSYS argument rewriting
Unix -> Windows path guessing
mintty compatibility shim
Cygwin process model
```

Git for Windows currently uses MSYS2 specifically because its Bash expects that POSIX environment; its documentation states that Git Bash's `bash.exe` ultimately runs a Bash depending on `msys-2.0.dll`. ([Git for Windows][6])

Your Bash would instead be a normal PE executable:

```text
bash.exe
    kernel32.dll
    ntdll.dll
    user32.dll maybe
    ...
```

with no `msys-2.0.dll`.

## Pipes and redirection are not particularly difficult

This:

```bash
foo | bar | baz > output.txt
```

maps reasonably well to Windows anonymous pipes and inherited process handles.

Windows lets `CreateProcess` explicitly inherit selected handles, and modern Windows provides `STARTUPINFOEX`/`PROC_THREAD_ATTRIBUTE_HANDLE_LIST` so you do not have to indiscriminately leak inheritable handles to children. ([Microsoft Learn][7])

So:

```bash
program >foo.txt
program 2>errors.txt
program <input.txt
a | b
```

are quite practical.

Internally you would probably implement a Bash FD table:

```text
Bash FD       Windows HANDLE

0             stdin HANDLE
1             stdout HANDLE
2             stderr HANDLE
3             some HANDLE
...
```

For Bash builtins and Bash child processes, arbitrary descriptors can work.

For arbitrary Windows programs, descriptors above `2` get trickier because Windows has no universal POSIX-fd ABI. But that limitation is unlikely to matter for ordinary native Windows executables.

## Terminals are much less of a problem now

Modern Windows gives you ConPTY.

ConPTY was explicitly designed to provide a pseudoterminal-like interface between terminal applications and console applications, using UTF-8 and virtual-terminal sequences. ([Microsoft Learn][8])

So a native Bash could run directly inside Windows Terminal:

```text
Windows Terminal
      |
    ConPTY
      |
 native bash.exe
      |
 native Windows processes
```

No mintty required.

Readline could speak ANSI/VT just as it does elsewhere.

This part is quite clean nowadays.

## Job control and signals are harder

Bash expects things like:

```bash
Ctrl+C
Ctrl+Z

jobs
fg
bg

kill -TERM ...
kill -STOP ...
```

Bash's own documentation shows how deeply POSIX signals and process groups are tied to interactive job control. ([GNU][9])

Windows does have process groups:

```text
CREATE_NEW_PROCESS_GROUP
```

and can send console control events such as Ctrl+Break to groups. ([Microsoft Learn][10])

So you could get good behavior for:

```text
Ctrl+C
foreground processes
background processes
wait
jobs
```

But there isn't a direct Windows equivalent of the whole Unix signal model.

For example, arbitrary native Windows applications have no native concept equivalent to:

```text
SIGSTOP
SIGTSTP
SIGCONT
SIGCHLD
SIGHUP
```

You can emulate some of these inside Bash itself.

You cannot make arbitrary `foo.exe` magically become POSIX-aware.

So this would probably be:

```bash
kill -TERM pid
```

mapped to some Windows termination behavior,

while:

```bash
kill -STOP pid
```

would either be unsupported or documented as Windows-specific behavior.

This is one area where 100% Bash compatibility is impossible without some form of compatibility runtime.

## There is another surprisingly nasty problem: `argv`

Unix essentially gives a child:

```c
int main(int argc, char **argv)
```

because `execve()` receives an actual array of arguments.

Windows `CreateProcessW()` receives a command-line string.

So Bash may internally have:

```text
argv[0] = "foo.exe"
argv[1] = "hello world"
argv[2] = "\"something\""
```

but it must convert that back into:

```text
foo.exe "hello world" "\"something\""
```

and the target program then parses that string again.

Different Windows runtimes can parse command lines differently.

That means there is no mathematically perfect:

```text
Bash argv[] -> arbitrary Windows application argv[]
```

conversion.

For programs using Microsoft's normal C runtime conventions, it is manageable. But it is an inherent mismatch you cannot completely eliminate.

## `$PATH` needs special treatment

This deserves explicit design rather than heuristic conversion.

Windows normally has:

```text
C:\Windows;C:\tools;D:\SDK\bin
```

whereas Unix has:

```text
/usr/bin:/bin:/opt/foo/bin
```

Using Unix `:` separators is impossible with ordinary Windows drive paths because:

```text
C:/foo:D:/bar
^      ^
```

is ambiguous.

So a native Bash should probably make:

```bash
PATH='C:/Windows/System32;C:/tools;D:/SDK/bin'
```

the canonical representation.

Then patch Bash's command-search code to understand semicolon-separated `PATH`.

For compatibility, it could recognize:

```bash
PATH=/usr/bin:/bin
```

when all entries clearly use Unix compatibility paths.

This does mean that scripts which manually parse `$PATH` assuming `:` are not portable to native Windows Bash.

There is no way around that without introducing another abstraction.

## Command lookup could feel very Windows-native

You could teach Bash:

```bash
foo
```

to search:

```text
foo.exe
foo.com
foo.cmd
foo.bat
foo
```

according to something similar to `PATHEXT`.

Native executables:

```text
.exe
.com
```

would use `CreateProcessW()` directly.

A `.bat` or `.cmd` file could transparently invoke:

```text
cmd.exe /d /s /c ...
```

without making `cmd.exe` your interactive shell.

Shell scripts could work too:

```bash
#!/usr/bin/env bash
```

because Bash itself can recognize the shebang when doing command resolution. Windows does not have to understand it.

You could therefore have:

```text
foo.exe
build.sh
configure
something.cmd
```

all callable from the same Bash prompt.

That would be genuinely nice.

## The filesystem semantics will never be exactly Unix

A few incompatibilities remain because they are characteristics of Windows rather than Bash:

```text
case-insensitive filenames by default
different symlink semantics
no traditional Unix executable bit
Windows ACLs instead of Unix mode bits
reserved names such as NUL, CON, COM1
different file locking semantics
different delete/open-file semantics
different environment semantics
drive letters
UNC paths
```

For example:

```bash
chmod +x foo
```

doesn't really have a native Windows meaning.

You could make Bash treat:

```text
.exe
.com
.cmd
.bat
```

and shebang-containing files as executable instead.

That is probably preferable to inventing an invisible POSIX permission database.

## So there are really two different goals

If the goal is:

> Run arbitrary Linux Bash scripts exactly as on Linux.

Then I would say no: a pure Win32 implementation cannot give you that without effectively recreating a POSIX environment. Eventually you reinvent Cygwin, WSL1, or something similar.

But if the goal is:

> Give me Bash language and Bash interactive behavior as a first-class Windows shell, where Windows paths, Windows executables, Windows Terminal, Windows environment variables and Win32 processes are the native model.

Then yes.

That is entirely feasible.

And I think that distinction is important.

You would be building something like:

```text
                 GNU Bash language
                        |
           +------------+-------------+
           |                          |
      Bash parser                 Bash builtins
      expansions                  functions
      variables                   readline
           |                          |
           +------------+-------------+
                        |
                Win32 adaptation
                        |
       +--------+-------+-------+---------+
       |        |               |         |
     paths   processes         I/O      terminal
       |        |               |         |
 CreateFileW CreateProcessW   HANDLE    ConPTY
 FindFirst   Job/process      pipes     VT/UTF-8
 FileW       groups
```

Not:

```text
Bash
  |
fake Unix
  |
Windows
```

but:

```text
Bash
  |
Windows
```

That is a very different architecture from MSYS2.

And it explains why I would call such a thing "Windows Bash" rather than "Bash running in a Unix environment on Windows."

The GNU project itself describes Bash as highly portable and notes independently supported Windows ports, so the parser/language itself is not fundamentally tied to Linux. The difficult parts are the OS-facing semantics surrounding the language. ([GNU][11])

As of 2026, MSYS2 is shipping a current Bash 5.3 build, but still specifically as an MSYS package, which illustrates the current state: maintaining Bash itself on Windows is routine; removing the POSIX substrate is the part nobody currently appears to maintain as a serious modern project. ([MSYS2 Packages][12])

If you were actually considering implementing this, the interesting route would be to start with Bash 5.3 and replace the process/subshell layer first. Path handling is comparatively easy. Getting `$(...)`, pipelines containing shell builtins, `( ... )`, background jobs, traps, Ctrl+C and state inheritance correct without `fork()` is where most of the engineering work would be.

[1]: https://sourceforge.net/projects/win-bash/?utm_source=chatgpt.com "win-bash download | SourceForge.net"
[2]: https://learn.microsoft.com/en-us/windows/wsl/wsl2-about?utm_source=chatgpt.com "Comparing WSL Versions | Microsoft Learn"
[3]: https://www.msys2.org/docs/filesystem-paths/?utm_source=chatgpt.com "Filesystem Paths - MSYS2"
[4]: https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file?utm_source=chatgpt.com "Naming Files, Paths, and Namespaces - Win32 apps | Microsoft Learn"
[5]: https://gitforwindows.org/problems-specific-to-32-bit.html?utm_source=chatgpt.com "Problems specific to 32-bit"
[6]: https://gitforwindows.org/requirements.html?utm_source=chatgpt.com "requirements"
[7]: https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessa?utm_source=chatgpt.com "CreateProcessA function (processthreadsapi.h) - Win32 apps | Microsoft Learn"
[8]: https://learn.microsoft.com/en-us/windows/console/pseudoconsoles?utm_source=chatgpt.com "Pseudoconsoles – Windows Desktop - Windows Console | Microsoft Learn"
[9]: https://www.gnu.org/software/bash/manual/html_node/Signals.html?pubDate=20260131&utm_source=chatgpt.com "Signals (Bash Reference Manual)"
[10]: https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags?utm_source=chatgpt.com "Process Creation Flags (WinBase.h) - Win32 apps | Microsoft Learn"
[11]: https://www.gnu.org/software/bash/?utm_source=chatgpt.com "Bash - GNU Project - Free Software Foundation"
[12]: https://packages.msys2.org/packages/bash?utm_source=chatgpt.com "Package: bash - MSYS2 Packages"


So I would combine my Cool Again Shell or cash with Scoop executables.

Yes. `cash` plus Scoop is a much cleaner design than trying to make Git Bash or MSYS2 feel native.

I would make the boundary very explicit:

```text
Windows Terminal
      |
    cash.exe
      |
      +-- shell language / parser / expansion
      +-- pipes / redirection / jobs
      +-- Windows path semantics
      +-- CreateProcessW
      |
      +-- Scoop shims
              |
              +-- rg.exe
              +-- fd.exe
              +-- jq.exe
              +-- git.exe
              +-- python.exe
              +-- node.exe
              +-- ...
```

Scoop already gives you most of the executable-distribution problem for free. Its manifests declare programs through the `bin` property, and Scoop exposes those programs through its `shims` directory. ([GitHub][1])

Even better, modern Scoop's executable shim mechanism is conceptually very compatible with what `cash` would want. A `.shim` file is basically metadata like:

```text
path = C:\Users\me\scoop\apps\ripgrep\current\rg.exe
args = ...
cwd = ...
```

and Scoop's shim launcher runs the target executable and forwards arguments and exit status. The current shim project even has a native C++ implementation. ([GitHub][2])

So `cash` does not need to understand Scoop packages at all initially. It can simply have:

```text
C:\Users\me\scoop\shims
```

in its `PATH` and execute:

```bash
rg foo .
fd '\.rs$'
jq '.foo' data.json
git status
python app.py
```

as ordinary Windows processes.

## I would go one step further, though

Eventually I would teach `cash` about Scoop shims directly.

Instead of:

```text
cash
  -> rg.exe shim
      -> actual rg.exe
```

you could optionally do:

```text
cash
  -> read rg.shim
  -> CreateProcessW(actual rg.exe)
```

That removes one process layer and gives `cash` better knowledge of the actual command.

The shim format is sufficiently simple that this would not be difficult. Scoop currently defines fields for the target path, preset arguments, working directory, elevation, and environment-variable overrides. ([GitHub][2])

I would not make that a requirement, however. Supporting normal Windows `PATH` lookup first is much healthier:

```text
cash command lookup:

1. builtin/function/alias
2. current command cache
3. search PATH
4. honor PATHEXT
5. CreateProcessW
```

That means `cash` remains independent of Scoop.

Scoop is merely an excellent default package source.

## This also gives you a very good philosophy for `cash`

Something like:

> Bash-like shell semantics, Windows-native execution.

Not:

> Unix emulation for Windows.

For example:

```bash
cd C:/src/kernel

rg 'TODO' .
git status

mkdir build
cmake -S . -B build
ninja -C build

python scripts/test.py | jq '.results[]'
```

All of those processes can be genuine Windows executables.

No:

```text
/c/src
/cygdrive/c
/mnt/c
MSYS2_ARG_CONV_EXCL
wsl.exe
```

required.

I think `C:/foo/bar` should simply be the preferred spelling in Cash. Win32 APIs accept forward slashes in many ordinary filesystem contexts, while Cash itself could normalize paths before passing them into filesystem APIs where necessary.

So:

```bash
HOME=C:/Users/piet
CACHE=C:/Users/piet/AppData/Local/cache
PROJECT=D:/src/project
```

would be normal Cash.

## Scoop itself is the one slightly awkward piece

Scoop's package-management implementation is PowerShell based. Its current documented prerequisites still require PowerShell 5.1 or later. ([GitHub][3])

But I would consider that completely acceptable.

You said you do not care about PowerShell. That does not necessarily mean PowerShell cannot exist as an implementation detail.

From Cash:

```bash
scoop install ripgrep
scoop install fd
scoop install jq
```

could internally end up invoking Scoop's PowerShell machinery.

You never interact with PowerShell syntax.

It is no different conceptually from:

```bash
npm install
```

running JavaScript internally.

You could even eventually wrap it:

```bash
cash pkg install ripgrep
cash pkg update
cash pkg search llvm
```

with:

```text
cash pkg
   |
   +-- Scoop backend
```

while still allowing:

```bash
scoop install ripgrep
```

for people who know Scoop.

I would resist implementing your own package manager. Scoop already has the manifests, buckets, update machinery, package version handling and a large existing ecosystem. Its bucket model is deliberately just repositories of JSON manifests. ([GitHub][4])

## There is one important distinction to make

"Scoop package" does not necessarily mean "native executable."

Scoop can expose:

```text
.exe
.cmd
.bat
.ps1
other scripts
```

because manifest `bin` entries can point to executables or scripts. ([GitHub][1])

So Cash should understand what it finds.

I would probably classify commands internally:

```text
NativeExe
BatchScript
PowerShellScript
CashScript
ShebangScript
ScoopShim
Unknown
```

Then dispatch accordingly:

```text
foo.exe
    -> CreateProcessW

foo.cmd / foo.bat
    -> cmd.exe /d /s /c ...

foo.ps1
    -> pwsh.exe or powershell.exe

foo.cash
    -> cash interpreter

foo
    -> inspect shebang if appropriate
```

That way the shell itself is native even though it can interoperate with Windows' other script formats.

## You could also define a "Cash native" package expectation

This could become useful later.

For example:

```bash
cash pkg info ripgrep

Name:       ripgrep
Provider:   scoop
Command:    rg
Type:       NativeExe
Target:     C:/Users/me/scoop/apps/ripgrep/current/rg.exe
Architecture: x64
```

Versus:

```bash
cash pkg info something

Name:       something
Provider:   scoop
Command:    something
Type:       PowerShellScript
```

Then users who want an entirely native CLI environment could ask for only:

```text
NativeExe
```

packages.

You would not need a separate repository. Cash could inspect existing Scoop metadata.

## Scoop's `current` junction model also fits this nicely

Scoop installs versioned directories and maintains a `current` junction pointing at the active version:

```text
apps/
  ripgrep/
    14.1.1/
    current -> 14.1.1
```

That allows stable target paths across upgrades. ([GitHub][5])

So Cash could cache resolved commands without tying itself permanently to:

```text
...\ripgrep\14.1.1\rg.exe
```

It can either execute the Scoop shim or resolve through:

```text
...\ripgrep\current\rg.exe
```

That is a surprisingly good fit for a shell command cache.

## I would therefore divide Cash into three layers

```text
+--------------------------------------------------+
|                    Cash language                  |
|                                                  |
| variables, quoting, globbing, pipelines,         |
| functions, conditionals, loops, aliases          |
+--------------------------------------------------+
                         |
+--------------------------------------------------+
|               Windows shell runtime              |
|                                                  |
| PATH/PATHEXT       CreateProcessW                |
| HANDLE pipes       environment blocks            |
| ConPTY             process groups                |
| filesystem         job management                |
+--------------------------------------------------+
                         |
+--------------------------------------------------+
|                command ecosystem                 |
|                                                  |
| Scoop      WinGet      system executables        |
| Cargo      npm         Python                    |
| Go         user binaries                          |
+--------------------------------------------------+
```

And critically:

```text
Scoop != Cash
```

Cash should work perfectly with:

```text
C:\Windows\System32
C:\Program Files\...
C:\tools
Cargo binaries
Go binaries
npm binaries
random .exe files
```

Scoop would simply give you the really nice default experience.

## It might actually solve one of the things I dislike about many Windows shell approaches

A lot of them start with:

> How can Windows pretend to be Unix?

Cash could instead start from:

> How much of the Bash interaction model is useful independent of Unix?

A surprising amount:

```bash
foo | bar
foo > output
foo && bar
foo || bar
$(command)
${variable}
for x in ...
if ...
while ...
functions
aliases
globbing
history
readline-style editing
```

None of those inherently require Linux paths.

Then you deliberately adapt the bits that are OS-specific:

```text
process spawning
signals
job control
filesystem paths
permissions
executability
environment variables
PATH
```

to Windows rather than emulating Unix.

And Scoop supplies exactly the kind of CLI ecosystem that makes that useful immediately.

So yes: **Cash + Scoop-native tools is probably the architecture I would pursue.**

The important design constraint I would impose from day one is:

```text
Cash must never require Scoop.
Scoop must just happen to be an exceptionally good Cash package backend.
```

That keeps the shell architecturally clean while allowing a default installation story as simple as:

```bash
scoop install cash
cash

cash> scoop install ripgrep fd jq bat git
cash> rg foo .
```

At that point you would have something Windows does not really have today: a Bash-style command language whose fundamental execution model is Win32 rather than POSIX emulation.

[1]: https://github.com/ScoopInstaller/Scoop/wiki/App-Manifests?utm_source=chatgpt.com "App Manifests · ScoopInstaller/Scoop Wiki · GitHub"
[2]: https://github.com/ScoopInstaller/Shim?utm_source=chatgpt.com "GitHub - ScoopInstaller/Shim: A Scoop helper program for shimming executables · GitHub"
[3]: https://github.com/ScoopInstaller/Scoop/wiki/Quick-Start?utm_source=chatgpt.com "Quick Start · ScoopInstaller/Scoop Wiki · GitHub"
[4]: https://github.com/ScoopInstaller/Scoop/wiki/Buckets?utm_source=chatgpt.com "Buckets · ScoopInstaller/Scoop Wiki · GitHub"
[5]: https://github.com/ScoopInstaller/Scoop/wiki/The-%27Current%27-Version-Alias/625b29be9c0ff72843942667465a6c05fe2c5d67?utm_source=chatgpt.com "The 'Current' Version Alias · ScoopInstaller/Scoop Wiki · GitHub"
