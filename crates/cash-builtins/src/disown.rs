use clap::Parser;
use std::io::Write;

use cash_core::{ExecutionResult, builtins, jobs};

/// Remove jobs from the shell's table of active jobs.
///
/// cash: half of what bash's `disown` means cannot be delivered on Windows, and the other
/// half is the half scripts actually use.
///
/// On Linux a disowned job outlives the shell: no `SIGHUP` is sent to it on exit, and the
/// orphan is reparented to init. Every process cash spawns is assigned to the session job
/// object (D6), and Windows offers no way to take a process back out of a job once it is
/// in one, so a disowned job still dies with the shell (§4 #23). `detach` (D45) starts a
/// process *outside* the job object, and is the only spelling that survives cash.
///
/// What `disown` does deliver is the bookkeeping: the shell forgets the job, so `jobs`
/// stops listing it, `wait` stops waiting for it, and `kill %1` no longer finds it — which
/// is what `cmd & disown` is reaching for in a script that does not want the shell
/// blocking on a long-lived child at the end.
#[derive(Parser)]
#[clap(disable_help_flag = true)]
pub(crate) struct DisownCommand {
    /// Keep the job rather than removing it (Windows has no SIGHUP to suppress).
    #[arg(short = 'h')]
    mark_only: bool,

    /// Operate on all jobs.
    #[arg(short = 'a')]
    all_jobs: bool,

    /// Operate only on running jobs.
    #[arg(short = 'r')]
    running_only: bool,

    /// Jobs to disown, named by job spec or by process ID.
    #[arg(allow_hyphen_values = true)]
    job_specs: Vec<String>,
}

/// A job the shell was asked to forget: one it owns, or one a subshell only sees.
#[derive(Clone, Copy)]
enum Target {
    Owned(usize),
    Inherited(usize),
}

impl builtins::Command for DisownCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        // `-a` and `-r` take the whole table. bash ignores any specs given alongside them,
        // so `disown -a %1` still clears everything.
        if self.all_jobs || self.running_only {
            for target in self.whole_table(context.shell.jobs()) {
                self.apply(context.shell.jobs_mut(), target);
            }
            return Ok(ExecutionResult::success());
        }

        // With no spec at all, bash takes the current job — and names it "current" when
        // there is none, rather than echoing a spec the caller never typed.
        if self.job_specs.is_empty() {
            let Some(target) = current_job(context.shell.jobs()) else {
                writeln!(
                    context.stderr(),
                    "{}: current: no such job",
                    context.command_name
                )?;
                return Ok(ExecutionResult::general_error());
            };

            self.apply(context.shell.jobs_mut(), target);
            return Ok(ExecutionResult::success());
        }

        let mut failed = false;
        for spec in &self.job_specs {
            let Some(target) = resolve(context.shell.jobs_mut(), spec) else {
                // bash warns before failing when the argument could not have been a job
                // spec in the first place — `disown foo`, and notably `disown %1 -h`,
                // because only the first word may carry options.
                if !spec.is_empty() && !spec.starts_with('%') && !is_pid_shaped(spec) {
                    writeln!(
                        context.stderr(),
                        "{}: warning: {spec}: job specification requires leading `%'",
                        context.command_name
                    )?;
                }

                writeln!(
                    context.stderr(),
                    "{}: {spec}: no such job",
                    context.command_name
                )?;
                failed = true;
                continue;
            };

            self.apply(context.shell.jobs_mut(), target);
        }

        if failed {
            return Ok(ExecutionResult::general_error());
        }

        Ok(ExecutionResult::success())
    }
}

impl DisownCommand {
    /// Every job `-a` or `-r` selects, the latter keeping only the running ones.
    fn whole_table(&self, jobs: &jobs::JobManager) -> Vec<Target> {
        let running_only = self.running_only;
        let owned = jobs
            .jobs
            .iter()
            .filter(|job| !running_only || matches!(job.state, jobs::JobState::Running))
            .map(|job| Target::Owned(job.id));
        let inherited = jobs
            .inherited()
            .iter()
            .filter(|snapshot| !running_only || matches!(snapshot.state, jobs::JobState::Running))
            .map(|snapshot| Target::Inherited(snapshot.id));

        owned.chain(inherited).collect()
    }

    /// Forget the job — or, under `-h`, leave it exactly where it is.
    ///
    /// cash: `-h` asks for the job to be kept but exempted from the `SIGHUP` the shell
    /// sends on exit. cash never sends one — Windows has no `SIGHUP`, and the session job
    /// object ends the process tree without asking (D6) — so there is nothing for the mark
    /// to suppress and nothing it could suppress it with. Keeping the job listed is the
    /// whole of what `-h` observably does in bash, and that part is honoured; the spec is
    /// still resolved first, so a bad one is reported rather than quietly accepted.
    fn apply(&self, jobs: &mut jobs::JobManager, target: Target) {
        if self.mark_only {
            return;
        }

        match target {
            Target::Owned(id) => {
                jobs.remove_job(id);
            }
            Target::Inherited(id) => {
                jobs.forget_inherited(id);
            }
        }
    }
}

/// The current job, whether the shell owns it or only inherited a view of it.
fn current_job(jobs: &jobs::JobManager) -> Option<Target> {
    if let Some(job) = jobs.current_job() {
        return Some(Target::Owned(job.id));
    }

    jobs.inherited()
        .iter()
        .find(|snapshot| matches!(snapshot.annotation, jobs::JobAnnotation::Current))
        .map(|snapshot| Target::Inherited(snapshot.id))
}

/// Resolves one `disown` argument: a job spec, or the process ID of a job's process.
fn resolve(jobs: &mut jobs::JobManager, spec: &str) -> Option<Target> {
    if let Some(id) = jobs.resolve_job_spec(spec).map(|job| job.id) {
        return Some(Target::Owned(id));
    }

    if let Ok(pid) = cash_core::int_utils::parse(spec, 10)
        && let Some(id) = jobs.resolve_pid(pid).map(|job| job.id)
    {
        return Some(Target::Owned(id));
    }

    resolve_inherited(jobs.inherited(), spec).map(Target::Inherited)
}

/// The same resolution against the parent's jobs, as a subshell sees them.
fn resolve_inherited(snapshots: &[jobs::JobSnapshot], spec: &str) -> Option<usize> {
    if let Some(remainder) = spec.strip_prefix('%') {
        let found = match remainder {
            "%" | "+" => snapshots
                .iter()
                .find(|s| matches!(s.annotation, jobs::JobAnnotation::Current)),
            "-" => snapshots
                .iter()
                .find(|s| matches!(s.annotation, jobs::JobAnnotation::Previous)),
            digits if is_pid_shaped(digits) => {
                let id = digits.parse::<usize>().ok()?;
                snapshots.iter().find(|s| s.id == id)
            }
            _ => None,
        };

        return found.map(|s| s.id);
    }

    let pid = cash_core::int_utils::parse(spec, 10).ok()?;
    snapshots.iter().find(|s| s.pid == Some(pid)).map(|s| s.id)
}

/// Whether the argument is all digits, and so was meant as a process ID rather than a
/// mistyped job spec. An empty argument counts, because bash does not warn about one.
fn is_pid_shaped(spec: &str) -> bool {
    spec.chars().all(|c| c.is_ascii_digit())
}
