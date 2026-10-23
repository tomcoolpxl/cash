//! Git's own tab completion, loaded the first time `git` is completed.
//!
//! Git Bash completes git's subcommands, branches and remotes with no setup: its
//! `/etc/profile.d/git-prompt.sh` sources Git for Windows' `git-completion.bash`. cash runs
//! that script as it is; this loads it when a word after `git` or `gitk` is first
//! completed and nothing else completes them: no `complete` spec, no `complete -D`
//! default, and no `_git` or `__git_main` the user defined (carapace, when installed,
//! answers before cash's own completion is asked at all).
//!
//! Loading happens once per session, found or not, and leaves the shell as it was but for
//! the script's functions, variables and two `complete` specs: no process is started (the
//! script is found beside `git.exe`, or through the `.shim` file of a Scoop shim), its
//! output is discarded, traps do not fire, and `$?`, `$_` and every option are put back,
//! so `set -x`, `set -e`, `set -u` and the user's aliases do not reach it.

use std::path::{Path, PathBuf};

use crate::{extensions, interp, openfiles};

/// The commands `git-completion.bash` completes.
const COMPLETED: [&str; 2] = ["git", "gitk"];

/// Functions a git completion of the user's own defines; with one defined, cash leaves
/// git's completion to them.
const OWN_COMPLETION: [&str; 2] = ["_git", "__git_main"];

/// The directories under a Git for Windows root holding its own programs and scripts, by
/// build: `mingw64`, `ucrt64` (the UCRT builds), `clang64`, `clangarm64` (ARM), `mingw32`.
const PREFIXES: [&str; 5] = ["mingw64", "ucrt64", "clang64", "clangarm64", "mingw32"];

impl<SE: extensions::ShellExtensions> crate::Shell<SE> {
    /// Loads Git's completion script if `command` is `git` or `gitk` with no completion
    /// of its own, and returns whether `command` has a completion spec now.
    pub(crate) async fn load_git_completion_for(&mut self, command: &str) -> bool {
        if self.git_completion_tried || !self.wants_git_completion(command) {
            return false;
        }
        self.git_completion_tried = true;
        let Some(script) = self
            .find_first_executable_in_path("git")
            .and_then(|git| completion_script(&git))
        else {
            return false;
        };
        self.source_quietly(&script).await;
        self.completion_config().get(command).is_some()
    }

    /// Whether `command` is one Git's script completes and nothing completes yet. Only
    /// an interactive shell loads it: a script's `compgen` gets what the script set up.
    fn wants_git_completion(&self, command: &str) -> bool {
        self.options.interactive
            && COMPLETED.contains(&command)
            && self.completion_config().get(command).is_none()
            && OWN_COMPLETION
                .iter()
                .all(|name| self.funcs().get(name).is_none())
    }

    /// Sources `script` leaving no trace but what it defines: output to the null device,
    /// traps held back, and the status, `$_` and options put back afterwards.
    async fn source_quietly(&mut self, script: &Path) {
        let saved_status = self.save_command_status();
        let saved_options = self.options.clone();
        self.options.print_commands_and_arguments = false;
        self.options.print_shell_input_lines = false;
        self.options.exit_on_nonzero_command_exit = false;
        self.options.treat_unset_variables_as_error = false;
        self.options.expand_aliases = false;

        let mut params = self.default_exec_params();
        params.process_group_policy = interp::ProcessGroupPolicy::SameProcessGroup;
        for fd in [
            openfiles::OpenFiles::STDIN_FD,
            openfiles::OpenFiles::STDOUT_FD,
            openfiles::OpenFiles::STDERR_FD,
        ] {
            if let Ok(null) = openfiles::null() {
                params.set_fd(fd, null);
            }
        }

        // N.B. Released by hand, as `call_completion_function` does: the result is kept,
        // never returned early with `?`.
        self.acquire_trap_delivery_block();
        let shown = PathBuf::from(self.shown_path(script));
        let result = self
            .source_script(&shown, std::iter::empty::<String>(), &params)
            .await;
        self.release_trap_delivery_block();
        if let Err(error) = result {
            tracing::debug!("couldn't load {}: {error}", script.display());
        }

        self.options = saved_options;
        self.restore_command_status(saved_status);
    }
}

/// Git for Windows' `git-completion.bash` for the `git.exe` at `git`, when it has one.
///
/// Git for Windows puts `git.exe` in `cmd`, `bin` and `<prefix>/bin` under its root, and
/// the script in `<prefix>/share/git/completion`. A Scoop shim, `shims\git.exe`, names
/// the program it starts in `shims\git.shim`.
fn completion_script(git: &Path) -> Option<PathBuf> {
    let git = shim_target(git).unwrap_or_else(|| git.to_path_buf());
    git.ancestors().skip(2).take(2).find_map(|root| {
        PREFIXES
            .iter()
            .map(|prefix| {
                root.join(prefix)
                    .join("share/git/completion/git-completion.bash")
            })
            .find(|script| script.is_file())
    })
}

/// The program a Scoop shim starts: the `path` line of the `.shim` file beside it,
/// `path = "C:\...\git.exe"`, quoted or not.
fn shim_target(program: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(program.with_extension("shim")).ok()?;
    text.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == "path").then(|| PathBuf::from(value.trim().trim_matches('"')))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Git for Windows root, `prefix` its build's directory, with the completion script.
    fn git_root(prefix: &str) -> (tempfile::TempDir, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let completion = root.path().join(prefix).join("share/git/completion");
        std::fs::create_dir_all(&completion).unwrap();
        let script = completion.join("git-completion.bash");
        std::fs::write(&script, "").unwrap();
        (root, script)
    }

    #[test]
    fn the_script_is_found_from_each_place_git_exe_lives() {
        let (root, script) = git_root("mingw64");
        for git in ["cmd/git.exe", "bin/git.exe", "mingw64/bin/git.exe"] {
            assert_eq!(
                completion_script(&root.path().join(git)),
                Some(script.clone()),
                "{git}"
            );
        }
    }

    #[test]
    fn the_ucrt_and_arm_builds_are_found_too() {
        for prefix in ["ucrt64", "clangarm64"] {
            let (root, script) = git_root(prefix);
            assert_eq!(
                completion_script(&root.path().join("cmd/git.exe")),
                Some(script)
            );
        }
    }

    #[test]
    fn a_scoop_shim_leads_to_the_git_it_starts() {
        let (root, script) = git_root("mingw64");
        let shims = tempfile::tempdir().unwrap();
        let target = root.path().join("cmd").join("git.exe");
        for line in [
            format!("path = \"{}\"", target.display()),
            format!("path = {}", target.display()),
        ] {
            std::fs::write(shims.path().join("git.shim"), format!("{line}\n")).unwrap();
            assert_eq!(
                completion_script(&shims.path().join("git.exe")),
                Some(script.clone()),
                "{line}"
            );
        }
    }

    #[test]
    fn a_git_that_is_not_git_for_windows_has_none() {
        let elsewhere = tempfile::tempdir().unwrap();
        assert_eq!(
            completion_script(&elsewhere.path().join("bin/git.exe")),
            None
        );
    }
}
