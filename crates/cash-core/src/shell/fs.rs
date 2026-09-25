//! Filesystem interaction in the shell.

use std::path::{Path, PathBuf};

use normalize_path::NormalizePath as _;

use crate::{
    ExecutionParameters, ShellFd,
    env::{EnvironmentLookup, EnvironmentScope},
    error, openfiles, pathsearch,
    sys::users,
    variables,
};

impl<SE: crate::extensions::ShellExtensions> crate::Shell<SE> {
    /// Sets the shell's current working directory to the given path.
    ///
    /// # Arguments
    ///
    /// * `target_dir` - The path to set as the working directory.
    pub fn set_working_dir(&mut self, target_dir: impl AsRef<Path>) -> Result<(), error::Error> {
        // cash (D3/D10): this is the first of two chokepoints. Accept every spelling a
        // script or a Windows tool might produce — `C:/src`, `C:\src`, `/c/src`, `/tmp`
        // — and store the canonical form, so `$PWD` renders correctly for free because
        // the rest of the shell simply echoes what was stored here.
        #[cfg(windows)]
        let abs_path = {
            let spelled = target_dir.as_ref().to_string_lossy().into_owned();
            let accepted = cash_win32::path::accept_path(&spelled);
            self.absolute_path(accepted.as_path())
        };
        #[cfg(not(windows))]
        let abs_path = self.absolute_path(target_dir.as_ref());

        match std::fs::metadata(&abs_path) {
            Ok(m) => {
                if !m.is_dir() {
                    return Err(error::ErrorKind::NotADirectory(abs_path).into());
                }
            }
            Err(e) => {
                return Err(e.into());
            }
        }

        // Normalize the path (but don't canonicalize it).
        let cleaned_path = abs_path.normalize();

        // cash (D3/D10): store the canonical spelling in the shell's own cwd state, not
        // just in `$PWD`. Anything reading `working_dir()` — the `pwd` builtin, relative
        // path resolution, the prompt — then renders correctly without each one having
        // to remember to convert.
        #[cfg(windows)]
        let cleaned_path = std::path::PathBuf::from(cash_win32::path::render(&cleaned_path));

        // cash (D3): render the one canonical spelling — drive letter, forward slashes.
        // `pwd` prints `C:/src/infra`, never `C:\src\infra`, because rendered paths
        // usually become arguments to native executables where the wrong spelling is
        // fatal rather than cosmetic: `terraform -chdir="$(pwd)/modules"` must work.
        #[cfg(windows)]
        let pwd = cash_win32::path::render(&cleaned_path);
        #[cfg(not(windows))]
        let pwd = cleaned_path.to_string_lossy().to_string();

        self.env.update_or_add(
            "PWD",
            variables::ShellValueLiteral::Scalar(pwd),
            |_| Ok(()),
            EnvironmentLookup::Anywhere,
            EnvironmentScope::Global,
        )?;
        let oldpwd = std::mem::replace(self.working_dir_mut(), cleaned_path);

        // cash (D3): `cd -` echoes $OLDPWD, so it must carry the same spelling as $PWD.
        #[cfg(windows)]
        let oldpwd = cash_win32::path::render(&oldpwd);
        #[cfg(not(windows))]
        let oldpwd = oldpwd.to_string_lossy().to_string();

        self.env.update_or_add(
            "OLDPWD",
            variables::ShellValueLiteral::Scalar(oldpwd),
            |_| Ok(()),
            EnvironmentLookup::Anywhere,
            EnvironmentScope::Global,
        )?;

        Ok(())
    }

    /// Tilde-shortens the given string, replacing the user's home directory with a tilde.
    ///
    /// # Arguments
    ///
    /// * `s` - The string to shorten.
    pub fn tilde_shorten(&self, s: String) -> String {
        if let Some(home_dir) = self.home_dir()
            && let Some(stripped) = s.strip_prefix(home_dir.to_string_lossy().as_ref())
        {
            return format!("~{stripped}");
        }
        s
    }

    /// Returns the shell's current home directory, if available.
    pub(crate) fn home_dir(&self) -> Option<PathBuf> {
        let home = if let Some(home) = self.env.get_str("HOME", self) {
            Some(PathBuf::from(home.to_string()))
        } else {
            // HOME isn't set, so let's sort it out ourselves.
            users::get_current_user_home_dir()
        };

        // cash (D3): one canonical spelling. Everything derived from the home directory
        // — `~` expansion, `$HISTFILE`, the default `.cashrc` path — inherits whatever
        // this returns, so rendering here keeps them all consistent rather than leaving
        // `$PWD` as `C:/Users/thraa` while `$HISTFILE` is `C:\Users\thraa\.cash_history`.
        #[cfg(windows)]
        let home = home.map(|h| PathBuf::from(cash_win32::path::render(&h)));

        home
    }

    /// Returns the directories named by `$PATH`, in search order, and whether any of
    /// them was relative.
    ///
    /// Empty entries mean `.` (Bash 5.3 also treats an empty `$PATH` this way). Relative
    /// entries are resolved against the shell's working directory rather than the
    /// process's, which `cd` does not change.
    fn executable_search_dirs(&self) -> (Vec<PathBuf>, bool) {
        let path_var = self.env.get_str("PATH", self).unwrap_or_default();
        let mut has_relative = false;
        let dirs = crate::sys::fs::split_paths_preserving_empty(path_var.as_ref())
            .map(|dir| {
                if dir.is_absolute() {
                    dir
                } else {
                    has_relative = true;
                    self.absolute_path(dir)
                }
            })
            .collect();
        (dirs, has_relative)
    }

    /// Finds executables with the given name in the shell's current PATH, yielding each match
    /// in search order.
    ///
    /// # Arguments
    ///
    /// * `filename` - The name of the executable to look for.
    pub fn find_executables_in_path<'a>(
        &'a self,
        filename: &'a str,
    ) -> impl Iterator<Item = PathBuf> + 'a {
        let (paths, _) = self.executable_search_dirs();

        pathsearch::search_for_executable(paths.into_iter(), filename)
    }

    /// Finds executables in the shell's current default PATH, with filenames matching the
    /// given prefix.
    ///
    /// # Arguments
    ///
    /// * `filename_prefix` - The prefix to match against executable filenames.
    pub fn find_executables_in_path_with_prefix(
        &self,
        filename_prefix: &str,
        case_insensitive: bool,
    ) -> impl Iterator<Item = PathBuf> {
        let (paths, _) = self.executable_search_dirs();

        pathsearch::search_for_executable_with_prefix(
            paths.into_iter(),
            filename_prefix,
            case_insensitive,
        )
    }

    /// Determines whether the given filename is the name of an executable in one of the
    /// directories in the shell's current PATH. If found, returns the path.
    ///
    /// # Arguments
    ///
    /// * `candidate_name` - The name of the file to look for.
    pub fn find_first_executable_in_path<S: AsRef<str>>(
        &self,
        candidate_name: S,
    ) -> Option<PathBuf> {
        self.find_executables_in_path(candidate_name.as_ref())
            .next()
    }

    /// Uses the shell's hash-based path cache to check whether the given filename is the name
    /// of an executable in one of the directories in the shell's current PATH. If found,
    /// ensures the path is in the cache and returns it.
    ///
    /// # Arguments
    ///
    /// * `candidate_name` - The name of the file to look for.
    pub fn find_first_executable_in_path_using_cache<S: AsRef<str>>(
        &mut self,
        candidate_name: S,
    ) -> Option<PathBuf>
    where
        String: From<S>,
    {
        if let Some(cached_path) = self.program_location_cache.get(&candidate_name) {
            Some(cached_path)
        } else if let Some(found_path) = self.find_first_executable_in_path(&candidate_name) {
            // A hit through a relative `$PATH` entry depends on the working directory, so
            // caching it would keep resolving to the old directory after a `cd`.
            if !self.executable_search_dirs().1 {
                self.program_location_cache
                    .set(candidate_name, found_path.clone());
            }
            Some(found_path)
        } else {
            None
        }
    }

    /// Resolves a command name by searching the shell's current PATH.
    ///
    /// Unlike [`Self::find_first_executable_in_path`], a non-executable entry in the PATH
    /// resolves as the command; the shell reports it as the command and then fails to run it.
    /// See [`pathsearch::resolve_command`].
    ///
    /// # Arguments
    ///
    /// * `candidate_name` - The name of the command to resolve.
    pub fn resolve_command_in_path<S: AsRef<str>>(&self, candidate_name: S) -> Option<PathBuf> {
        let (paths, _) = self.executable_search_dirs();
        pathsearch::resolve_command(paths, candidate_name.as_ref())
    }

    /// Like [`Self::resolve_command_in_path`], but consults the shell's hash-based path cache
    /// first and caches whatever a search turns up.
    ///
    /// # Arguments
    ///
    /// * `candidate_name` - The name of the command to resolve.
    pub fn resolve_command_in_path_using_cache<S: AsRef<str>>(
        &mut self,
        candidate_name: S,
    ) -> Option<PathBuf>
    where
        String: From<S>,
    {
        if let Some(cached_path) = self.program_location_cache.get(&candidate_name) {
            return Some(cached_path);
        }

        let found_path = self.resolve_command_in_path(candidate_name.as_ref())?;
        // See `find_first_executable_in_path_using_cache`.
        if !self.executable_search_dirs().1 {
            self.program_location_cache
                .set(candidate_name, found_path.clone());
        }

        Some(found_path)
    }

    /// Gets the absolute form of the given path.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to get the absolute form of.
    pub fn absolute_path(&self, path: impl AsRef<Path>) -> PathBuf {
        let path = path.as_ref();

        // cash (D3/D10): the second chokepoint. Every path the shell resolves funnels
        // through here, so accepting the Unix spellings once means `cat /c/Users/x`,
        // `> /c/tmp/out` and `[ -f /c/Windows/win.ini` all work without each call site
        // knowing about it. Relative paths and ordinary Windows paths pass through
        // unchanged.
        //
        // `/dev/null` and friends never reach this point — `open_file` intercepts them
        // first, because under D29's `\\?\` prefix `NUL` would name a file rather than
        // the device (D7, D28).
        #[cfg(windows)]
        let accepted = cash_win32::path::accept_path(&path.to_string_lossy());
        #[cfg(windows)]
        let path = accepted.as_path();

        if path.as_os_str().is_empty() || path.is_absolute() {
            return path.to_owned();
        }

        let joined = self.working_dir().join(path);

        // cash (D3): `PathBuf::join` inserts a backslash, so `cd /c/tmp; chmod +w f`
        // produced `C:/tmp\f` — a spelling Win32 accepts and D3 says cash never renders.
        // It reached the user through every diagnostic that names a resolved path.
        #[cfg(windows)]
        return PathBuf::from(cash_win32::path::render(&joined));
        #[cfg(not(windows))]
        joined
    }

    /// Opens the given file, using the context of this shell and the provided execution parameters.
    ///
    /// # Arguments
    ///
    /// * `options` - The options to use opening the file.
    /// * `path` - The path to the file to open; may be relative to the shell's working directory.
    /// * `params` - Execution parameters.
    pub(crate) fn open_file(
        &self,
        options: &std::fs::OpenOptions,
        path: impl AsRef<Path>,
        params: &ExecutionParameters,
    ) -> Result<openfiles::OpenFile, std::io::Error> {
        // Give platform-specific code a chance to handle special files
        // (e.g. /dev/null on Windows, which needs to open NUL instead).
        // This is checked before absolute_path so that paths like /dev/null
        // are intercepted on platforms where they aren't valid native paths.
        if let Some(result) = crate::sys::fs::try_open_special_file(path.as_ref()) {
            return result.map(openfiles::OpenFile::from);
        }

        let path_to_open = self.absolute_path(path.as_ref());

        // See if this is a reference to a file descriptor. These paths should
        // reflect the shell's current execution fds, which can differ from the
        // host process fds after redirections like here-docs.
        if let Some(fd_num) = shell_fd_path_to_fd(&path_to_open)
            && let Some(open_file) = params.try_fd(self, fd_num)
        {
            return Ok(open_file);
        }

        Ok(options.open(path_to_open)?.into())
    }

    /// Replaces the shell's currently configured open files with the given set.
    /// Typically only used by exec-like builtins.
    ///
    /// # Arguments
    ///
    /// * `open_files` - The new set of open files to use.
    pub fn replace_open_files(
        &mut self,
        open_fds: impl Iterator<Item = (ShellFd, openfiles::OpenFile)>,
    ) {
        self.open_files = openfiles::OpenFiles::from(open_fds);
    }

    pub(crate) const fn persistent_open_files(&self) -> &openfiles::OpenFiles {
        &self.open_files
    }
}

fn shell_fd_path_to_fd(path: &Path) -> Option<ShellFd> {
    match path.to_str()? {
        "/dev/stdin" => return Some(openfiles::OpenFiles::STDIN_FD),
        "/dev/stdout" => return Some(openfiles::OpenFiles::STDOUT_FD),
        "/dev/stderr" => return Some(openfiles::OpenFiles::STDERR_FD),
        _ => {}
    }

    if let Some(parent) = path.parent()
        && parent == Path::new("/dev/fd")
        && let Some(filename) = path.file_name()
    {
        filename.to_string_lossy().parse::<ShellFd>().ok()
    } else {
        None
    }
}
