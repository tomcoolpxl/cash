//! Filesystem interaction in the shell.

use std::path::{Path, PathBuf};

use normalize_path::NormalizePath as _;

use crate::{
    ExecutionParameters, ShellFd,
    env::{EnvironmentLookup, EnvironmentScope},
    error, openfiles, pathsearch,
    sys::{fs::PathExt as _, users},
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
        let abs_path = {
            let spelled = target_dir.as_ref().to_string_lossy().into_owned();
            let accepted = cash_win32::path::accept_path(&spelled);
            self.absolute_path(accepted.as_path())
        };

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
        let cleaned_path = std::path::PathBuf::from(cash_win32::path::render(&cleaned_path));

        // cash (D3): render the one canonical spelling — drive letter, forward slashes.
        // `pwd` prints `C:/src/infra`, never `C:\src\infra`, because rendered paths
        // usually become arguments to native executables where the wrong spelling is
        // fatal rather than cosmetic: `terraform -chdir="$(pwd)/modules"` must work.
        let pwd = cash_win32::path::render(&cleaned_path);

        self.env.update_or_add(
            "PWD",
            variables::ShellValueLiteral::Scalar(pwd),
            |_| Ok(()),
            EnvironmentLookup::Anywhere,
            EnvironmentScope::Global,
        )?;
        let oldpwd = std::mem::replace(self.working_dir_mut(), cleaned_path);

        // cash (D62): fish's folder history, for `prevd`, `nextd`, `cdh` and Alt-←/→.
        if oldpwd != *self.working_dir() {
            self.directory_history.left(oldpwd.clone());
        }

        // cash (D3): `cd -` echoes $OLDPWD, so it must carry the same spelling as $PWD.
        let oldpwd = cash_win32::path::render(&oldpwd);

        self.env.update_or_add(
            "OLDPWD",
            variables::ShellValueLiteral::Scalar(oldpwd),
            |_| Ok(()),
            EnvironmentLookup::Anywhere,
            EnvironmentScope::Global,
        )?;

        Ok(())
    }

    /// Moves `steps` places through the folder history (D62): back when negative, forward
    /// when positive, as `prevd` and `nextd` do. Returns the folder moved to, or `None`
    /// when the history does not reach that far; the history itself is only walked, not
    /// added to.
    ///
    /// # Arguments
    ///
    /// * `steps` - How far to move, and in which direction.
    pub fn step_directory_history(
        &mut self,
        steps: isize,
    ) -> Result<Option<PathBuf>, error::Error> {
        let Some(target) = self.directory_history.target(steps).map(Path::to_path_buf) else {
            return Ok(None);
        };
        let history = self.directory_history.clone();
        let current = self.working_dir().to_path_buf();
        self.set_working_dir(&target)?;
        self.directory_history = history;
        self.directory_history.moved(current, steps);
        Ok(Some(target))
    }

    /// Returns the folders the shell has been in (D62).
    pub const fn directory_history(&self) -> &crate::dirhistory::DirectoryHistory {
        &self.directory_history
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
        home.map(|h| PathBuf::from(cash_win32::path::render(&h)))
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

        pathsearch::search_for_executable(paths.into_iter(), filename, self.pathext())
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
            self.pathext(),
        )
    }

    /// Whether `name` is an executable on the shell's current PATH, answered from a
    /// background listing of the PATH directories; `None` while that listing is not ready.
    ///
    /// For callers that must not wait on the disk, such as syntax highlighting, which asks
    /// on every keystroke. The answer can lag a program installed moments ago, so nothing
    /// that runs a command should use it; see [`crate::pathindex`].
    ///
    /// # Arguments
    ///
    /// * `name` - The command name; it must not contain a path separator.
    pub fn executable_on_path_if_known(&self, name: &str) -> Option<bool> {
        if pathsearch::runs_cash_itself(name) {
            return Some(true);
        }
        let (dirs, _) = self.executable_search_dirs();
        self.path_index
            .contains_executable(&dirs, name, &self.pathext())
    }

    /// Whether a command spelled with a path names a file the shell can run: the path
    /// resolved as every path is (see [`Self::absolute_path`]), not a directory (which
    /// carries the execute bit but is never a command), and executable by the platform's
    /// rule, which on Windows includes `PATHEXT` and a runnable file's contents (D46).
    ///
    /// # Arguments
    ///
    /// * `name` - The command name, containing a path separator.
    pub fn is_runnable_path(&self, name: &str) -> bool {
        let candidate = self.absolute_path(Path::new(name));
        !candidate.is_dir() && candidate.executable(&self.pathext())
    }

    /// The file-creation mask `umask` reports. Windows has none to apply (D23), so it is
    /// only remembered; like Bash's it is the shell's own, and a subshell has a copy.
    pub const fn umask(&self) -> u32 {
        self.umask
    }

    /// Sets the file-creation mask `umask` reports.
    pub const fn set_umask(&mut self, umask: u32) {
        self.umask = umask;
    }

    /// Forgets the remembered command locations (`hash`) if `PATH` or `PATHEXT` changed
    /// since they were found, as Bash does when `PATH` is assigned: `PATH=a:$PATH; foo;
    /// PATH=b:$PATH; foo` ran `a/foo` twice.
    pub(crate) fn sync_program_location_cache(&mut self) {
        let basis = format!(
            "{}\0{}",
            self.env_str("PATH").unwrap_or_default(),
            self.env_str("PATHEXT").unwrap_or_default()
        );
        self.program_location_cache.forget_unless_found_with(&basis);
    }

    /// The extensions that make a file runnable by its bare name: the shell's `PATHEXT`,
    /// or Windows' default ones when it is unset or empty (D8).
    ///
    /// The shell's variable, not the process's: `export PATHEXT=.EXE` changes what cash
    /// finds as well as what its children do.
    pub fn pathext(&self) -> Vec<String> {
        cash_win32::resolve::pathext_of(self.env_str("PATHEXT").as_deref())
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
        pathsearch::resolve_command(paths, candidate_name.as_ref(), &self.pathext())
    }

    /// Like [`Self::resolve_command_in_path`], but consults the shell's hash-based path cache
    /// first and caches whatever a search turns up.
    ///
    /// This is the lookup made to *run* a command, so it counts a hit against the cached
    /// entry, which `hash` reports in its listing.
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
            self.program_location_cache.record_hit(&candidate_name);
            return Some(cached_path);
        }

        let found_path = self.resolve_command_in_path(candidate_name.as_ref())?;
        // See `find_first_executable_in_path_using_cache`.
        if !self.executable_search_dirs().1 {
            let name = String::from(candidate_name);
            self.program_location_cache
                .set(name.clone(), found_path.clone());
            self.program_location_cache.record_hit(name);
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
        // through here, so accepting the Unix spellings once means `ls /c/Users/x`,
        // `> /c/tmp/out` and `[ -f /c/Windows/win.ini` all work without each call site
        // knowing about it. Relative paths and ordinary Windows paths pass through
        // unchanged.
        //
        // `/dev/null` and friends do reach this point: a redirection resolves its word
        // before `open_file` sees it, and they leave as `C:/dev/null`. `open_file` tells
        // them in that spelling too, and opens no file for one, because under D29's
        // `\\?\` prefix `NUL` would name a file rather than the device (D7, D28).
        let accepted = cash_win32::path::accept_path(&path.to_string_lossy());
        let path = accepted.as_path();

        if path.as_os_str().is_empty() || path.is_absolute() {
            return path.to_owned();
        }

        let joined = self.working_dir().join(path);

        // cash (D3): `PathBuf::join` inserts a backslash, so `cd /c/tmp; chmod +w f`
        // produced `C:/tmp\f` — a spelling Win32 accepts and D3 says cash never renders.
        // It reached the user through every diagnostic that names a resolved path.
        PathBuf::from(cash_win32::path::render(&joined))
    }

    /// Opens the given file, using the context of this shell and the provided execution parameters.
    ///
    /// # Arguments
    ///
    /// * `options` - The options to use opening the file.
    /// * `access` - What the file is opened to do, as `options` say it; they cannot be asked.
    /// * `path` - The path to the file to open; may be relative to the shell's working directory.
    /// * `params` - Execution parameters.
    pub(crate) fn open_file(
        &self,
        options: &std::fs::OpenOptions,
        access: crate::sys::fs::Access,
        path: impl AsRef<Path>,
        params: &ExecutionParameters,
    ) -> Result<openfiles::OpenFile, std::io::Error> {
        // Give platform-specific code a chance to handle special files
        // (e.g. /dev/null on Windows, which needs to open NUL instead, and /dev/tty,
        // which is the console's input or its output). This is checked before
        // absolute_path so that paths like /dev/null are intercepted on platforms
        // where they aren't valid native paths.
        if let Some(result) = crate::sys::fs::try_open_special_file(path.as_ref(), access) {
            return result.map(openfiles::OpenFile::from);
        }

        let path_to_open = self.absolute_path(path.as_ref());

        // See if this is a reference to a file descriptor. These paths should
        // reflect the shell's current execution fds, which can differ from the
        // host process fds after redirections like here-docs.
        //
        // cash (D7): the name is told as `absolute_path` leaves it, `C:/dev/stderr`, and
        // one whose descriptor is not open is no file, in Bash's words. Going on to look
        // for it would find a file of that name in a folder of the user's own called
        // `C:\dev`, or with `>` make one.
        if let Some(fd_num) = crate::sys::fs::named_descriptor(&path_to_open) {
            return params.try_fd(self, fd_num).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "No such file or directory")
            });
        }

        // A process substitution's pipe cannot be created or truncated (D17).
        if !matches!(access, crate::sys::fs::Access::Read) {
            return Ok(cash_win32::pipe::open_output(options, &path_to_open)?.into());
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
