//! Init script support for shells.

use std::path::PathBuf;

use crate::{Shell, error, extensions, interp};

/// Behavior for loading profile files.
#[derive(Default)]
pub enum ProfileLoadBehavior {
    /// Load the default profile files.
    #[default]
    LoadDefault,
    /// Skip loading profile files.
    Skip,
}

impl ProfileLoadBehavior {
    /// Returns whether profile loading should be skipped.
    pub const fn skip(&self) -> bool {
        matches!(self, Self::Skip)
    }
}

/// Behavior for loading rc files.
#[derive(Default)]
pub enum RcLoadBehavior {
    /// Load the default rc files.
    #[default]
    LoadDefault,
    /// Load a custom rc file; do not load defaults.
    LoadCustom(PathBuf),
    /// Skip loading rc files.
    Skip,
}

impl RcLoadBehavior {
    /// Returns whether rc loading should be skipped.
    pub const fn skip(&self) -> bool {
        matches!(self, Self::Skip)
    }
}

impl<SE: extensions::ShellExtensions> Shell<SE> {
    /// Loads and executes standard shell configuration files (i.e., rc and profile), and then
    /// loads the history file (if command history is enabled). History is loaded last since
    /// the configuration files may change `HISTFILE`.
    ///
    /// # Arguments
    ///
    /// * `profile_behavior` - Behavior for loading profile files.
    /// * `rc_behavior` - Behavior for loading rc files.
    pub async fn load_config(
        &mut self,
        profile_behavior: &ProfileLoadBehavior,
        rc_behavior: &RcLoadBehavior,
    ) -> Result<(), error::Error> {
        self.load_config_files(profile_behavior, rc_behavior)
            .await?;

        // As bash does, skip the file if startup files already added entries (e.g. via
        // `history -s`). Do NOT fail if we can't load history.
        if self.options.enable_command_history
            && self
                .history
                .as_ref()
                .is_none_or(crate::history::History::is_empty)
            && let Ok(Some(history)) = self.load_history()
        {
            self.history = Some(history);
        }

        Ok(())
    }

    /// Define the `bash-completion` helper functions that generated completion scripts
    /// expect to find — **D40**.
    ///
    /// `docker completion bash`, `kubectl completion bash`, `gh completion -s bash` and
    /// every other Cobra-generated script call `_get_comp_words_by_ref` and `_filedir`.
    /// Those live in the `bash-completion` package rather than in bash, and on Windows
    /// there is nothing to install — Git for Windows does not ship it either, which is
    /// why sourcing `docker completion bash` there succeeds and then completes nothing
    /// at all. Defining the helpers is the whole fix.
    ///
    /// Failure is not fatal. A shell that starts without tab completion for `docker` is
    /// a worse shell; a shell that refuses to start is not a shell.
    #[cfg(windows)]
    pub(crate) async fn load_completion_shims(&mut self) -> Result<(), error::Error> {
        const SHIMS: &str = include_str!("../completion_shims.sh");

        let mut params = self.default_exec_params();
        params.process_group_policy = interp::ProcessGroupPolicy::SameProcessGroup;

        let source_info = crate::SourceInfo::from("cash: completion shims");
        if let Err(e) = self.run_string(SHIMS, &source_info, &params).await {
            tracing::warn!("failed to define completion compatibility shims: {e}");
        }

        Ok(())
    }

    async fn load_config_files(
        &mut self,
        profile_behavior: &ProfileLoadBehavior,
        rc_behavior: &RcLoadBehavior,
    ) -> Result<(), error::Error> {
        let mut params = self.default_exec_params();
        params.process_group_policy = interp::ProcessGroupPolicy::SameProcessGroup;

        if self.options.login_shell {
            // --noprofile means skip this.
            if matches!(profile_behavior, ProfileLoadBehavior::Skip) {
                return Ok(());
            }

            //
            // Source the system profile if it exists.
            //
            // Next source the first of these that exists and is readable (if any):
            //     * ~/.bash_profile
            //     * ~/.bash_login
            //     * ~/.profile
            //
            if let Some(system_profile) = crate::sys::fs::get_system_profile_path() {
                self.source_if_exists(system_profile, &params).await?;
            }
            if let Some(home_path) = self.home_dir() {
                if self.options.sh_mode {
                    self.source_if_exists(home_path.join(".profile").as_path(), &params)
                        .await?;
                } else {
                    if !self
                        .source_if_exists(home_path.join(".bash_profile").as_path(), &params)
                        .await?
                    {
                        if !self
                            .source_if_exists(home_path.join(".bash_login").as_path(), &params)
                            .await?
                        {
                            self.source_if_exists(home_path.join(".profile").as_path(), &params)
                                .await?;
                        }
                    }
                }
            }
        } else {
            if self.options.interactive {
                match rc_behavior {
                    _ if self.options.sh_mode => (),
                    RcLoadBehavior::Skip => (),
                    RcLoadBehavior::LoadCustom(rc_file) => {
                        // If an explicit rc file is provided, source it.
                        self.source_if_exists(rc_file, &params).await?;
                    }
                    RcLoadBehavior::LoadDefault => {
                        //
                        // Otherwise, for non-login interactive shells, load in this order:
                        //
                        //     system rc file (e.g. /etc/bash.bashrc on Unix)
                        //     ~/.bashrc
                        //
                        if let Some(system_rc) = crate::sys::fs::get_system_rc_path() {
                            self.source_if_exists(system_rc, &params).await?;
                        }
                        if let Some(home_path) = self.home_dir() {
                            self.source_if_exists(home_path.join(".bashrc").as_path(), &params)
                                .await?;
                            self.source_if_exists(home_path.join(".cashrc").as_path(), &params)
                                .await?;
                        }
                    }
                }
            } else {
                let env_var_name = if self.options.sh_mode {
                    "ENV"
                } else {
                    "BASH_ENV"
                };

                if self.env.is_set(env_var_name) {
                    //
                    // TODO(well-known-vars): look at $ENV/BASH_ENV; source its expansion if that
                    // file exists
                    //
                    return error::unimp(
                        "load config from $ENV/BASH_ENV for non-interactive, non-login shell",
                    );
                }
            }
        }

        Ok(())
    }
}
