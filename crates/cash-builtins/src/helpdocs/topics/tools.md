---
title: Tools to install: winget and Scoop names for common commands
summary: The package that installs a command cash cannot find, by its winget id and its Scoop name.
see: installing
---
## The hint at the prompt

When a command typed at the prompt is not found and is in the table below, cash follows
Bash's `jq: command not found` with one line, `install it: winget install jqlang.jq, or
scoop install jq`, so the name to install is at hand without a search. Scripts, `cash -c`
and a defined `command_not_found_handle` get Bash's message and status 127 and nothing
more, and `cash doctor` names the same packages for what it finds missing.

winget comes with Windows; Scoop is installed with `irm get.scoop.sh | iex` in PowerShell.
A name with a bucket, `extras/vscode`, is `scoop bucket add extras` first. A `-` means the
manager has no package for it.

## The table

| command | winget | scoop | what it is |
|---|---|---|---|
| jq | jqlang.jq | jq | JSON processor |
| yq | MikeFarah.yq | yq | YAML, JSON and XML processor |
| rg | BurntSushi.ripgrep.MSVC | ripgrep | ripgrep, a fast recursive grep |
| fd | sharkdp.fd | fd | a simpler find |
| fzf | junegunn.fzf | fzf | fuzzy finder |
| bat | sharkdp.bat | bat | cat with syntax highlighting |
| delta | dandavison.delta | delta | syntax-highlighting pager for git diffs |
| eza | eza-community.eza | eza | a modern ls |
| lsd | lsd-rs.lsd | lsd | ls with icons |
| zoxide | ajeetdsouza.zoxide | zoxide | a smarter cd |
| starship | Starship.Starship | starship | cross-shell prompt |
| direnv | direnv.direnv | direnv | per-folder environment variables |
| gh | GitHub.cli | gh | GitHub's command line |
| git | Git.Git | git | Git for Windows |
| lazygit | JesseDuffield.lazygit | extras/lazygit | terminal UI for git |
| node | OpenJS.NodeJS.LTS | nodejs-lts | Node.js, the LTS release |
| npm | OpenJS.NodeJS.LTS | nodejs-lts | comes with Node.js |
| pnpm | pnpm.pnpm | pnpm | package manager for Node.js |
| yarn | Yarn.Yarn | yarn | package manager for Node.js |
| bun | Oven-sh.Bun | bun | JavaScript runtime and package manager |
| deno | DenoLand.Deno | deno | JavaScript and TypeScript runtime |
| python | Python.Python.3.13 | python | Python 3 |
| python3 | Python.Python.3.13 | python | Python 3, installed as `python` |
| pip | Python.Python.3.13 | python | comes with Python |
| uv | astral-sh.uv | uv | Python package and project manager |
| ruby | RubyInstallerTeam.Ruby.3.3 | ruby | Ruby |
| perl | StrawberryPerl.StrawberryPerl | perl | Strawberry Perl |
| go | GoLang.Go | go | Go |
| rustup | Rustlang.Rustup | rustup | the Rust toolchain installer |
| cargo | Rustlang.Rustup | rustup | comes with rustup |
| dotnet | Microsoft.DotNet.SDK.10 | dotnet-sdk | .NET SDK |
| java | Microsoft.OpenJDK.21 | java/openjdk | a JDK |
| mvn | - | maven | Apache Maven |
| gradle | - | gradle | Gradle |
| docker | Docker.DockerDesktop | docker | Docker Desktop; Scoop's is the CLI and engine for Windows containers |
| podman | RedHat.Podman | podman | Podman containers |
| kubectl | Kubernetes.kubectl | kubectl | the Kubernetes command line |
| helm | Helm.Helm | helm | Kubernetes package manager |
| k9s | Derailed.k9s | k9s | terminal UI for Kubernetes |
| k3d | k3d.k3d | k3d | k3s clusters in Docker |
| terraform | Hashicorp.Terraform | terraform | Terraform |
| aws | Amazon.AWSCLI | aws | AWS command line |
| az | Microsoft.AzureCLI | azure-cli | Azure command line |
| gcloud | Google.CloudSDK | extras/gcloud | Google Cloud command line |
| make | ezwinports.make | make | GNU make |
| cmake | Kitware.CMake | cmake | CMake |
| ninja | Ninja-build.Ninja | ninja | the Ninja build system |
| gcc | BrechtSanders.WinLibs.POSIX.UCRT | mingw | GCC, as MinGW-w64 (WinLibs) |
| clang | LLVM.LLVM | llvm | LLVM and Clang |
| 7z | 7zip.7zip | 7zip | 7-Zip |
| zip | GnuWin32.Zip | zip | Info-ZIP's zip |
| unzip | GnuWin32.UnZip | unzip | Info-ZIP's unzip |
| gzip | - | gzip | GNU gzip |
| wget | JernejSimoncic.Wget | wget | GNU wget |
| aria2c | aria2.aria2 | aria2 | aria2 download utility |
| rclone | Rclone.Rclone | rclone | rsync for cloud storage |
| nano | GNU.Nano | nano | GNU nano editor |
| vim | vim.vim | vim | Vim |
| nvim | Neovim.Neovim | neovim | Neovim |
| emacs | GNU.Emacs | extras/emacs | GNU Emacs |
| hx | Helix.Helix | helix | the Helix editor |
| code | Microsoft.VisualStudioCode | extras/vscode | Visual Studio Code |
| shellcheck | koalaman.shellcheck | shellcheck | shell script linter |
| shfmt | mvdan.shfmt | shfmt | shell script formatter |
| ffmpeg | Gyan.FFmpeg | ffmpeg | FFmpeg |
| magick | ImageMagick.ImageMagick | imagemagick | ImageMagick |
| pandoc | JohnMacFarlane.Pandoc | pandoc | document converter |
| hugo | Hugo.Hugo.Extended | hugo-extended | Hugo static site generator |
| protoc | Google.Protobuf | extras/protobuf | Protocol Buffers compiler |
| sqlite3 | SQLite.SQLite | sqlite | SQLite shell |
| psql | PostgreSQL.PostgreSQL.18 | postgresql | PostgreSQL, with its client |
| mysql | Oracle.MySQL | mysql | MySQL, with its client |
| nmap | Insecure.Nmap | nmap | Nmap network scanner |
| openssl | FireDaemon.OpenSSL | openssl | OpenSSL |
| gpg | GnuPG.GnuPG | gnupg | GnuPG |
| just | Casey.Just | just | a command runner |
| pwsh | Microsoft.PowerShell | pwsh | PowerShell 7 |
| patch | - | patch | GNU patch |
