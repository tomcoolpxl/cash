//! `pstree` over the same native Windows process snapshot as `ps` and `top`.
//!
//! The process parent links are already available from Tool Help, so rendering them as a
//! tree adds no per-process queries. Command lines and thread trees are deliberately out
//! of scope; the executable name and native PID are the reliable cross-process fields.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Write;

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// Display native Windows processes as a parent/child tree.
#[derive(Parser)]
pub(crate) struct PsTreeCommand {
    /// Include each native process ID after its executable name.
    #[arg(short = 'p')]
    show_pids: bool,

    /// Start at this process rather than showing the complete machine forest.
    #[arg(value_name = "PID")]
    root: Option<u32>,
}

impl builtins::Command for PsTreeCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let processes: BTreeMap<u32, cash_win32::process::ProcessInfo> =
            cash_win32::process::list()
                .into_iter()
                .map(|process| (process.pid, process))
                .collect();

        if let Some(root) = self.root
            && !processes.contains_key(&root)
        {
            writeln!(context.stderr(), "pstree: process {root} was not found")?;
            return Ok(ExecutionResult::general_error());
        }

        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        for process in processes.values() {
            if process.parent_pid != process.pid && processes.contains_key(&process.parent_pid) {
                children
                    .entry(process.parent_pid)
                    .or_default()
                    .push(process.pid);
            }
        }
        for child_list in children.values_mut() {
            child_list.sort_unstable();
        }

        let mut stdout = context.stdout();
        let mut renderer = Renderer {
            out: &mut stdout,
            processes: &processes,
            children: &children,
            show_pids: self.show_pids,
            visited: HashSet::new(),
        };

        if let Some(root) = self.root {
            renderer.node(root, "", None)?;
        } else {
            let roots: Vec<u32> = processes
                .values()
                .filter(|process| {
                    process.parent_pid == process.pid
                        || !processes.contains_key(&process.parent_pid)
                })
                .map(|process| process.pid)
                .collect();
            for root in roots {
                renderer.node(root, "", None)?;
            }

            // PID reuse can briefly create a parent cycle. Keep those processes visible
            // as additional roots rather than dropping them from the listing.
            for pid in processes.keys().copied() {
                if !renderer.visited.contains(&pid) {
                    renderer.node(pid, "", None)?;
                }
            }
        }

        Ok(ExecutionResult::success())
    }
}

struct Renderer<'a, W> {
    out: &'a mut W,
    processes: &'a BTreeMap<u32, cash_win32::process::ProcessInfo>,
    children: &'a HashMap<u32, Vec<u32>>,
    show_pids: bool,
    visited: HashSet<u32>,
}

impl<W: Write> Renderer<'_, W> {
    fn node(
        &mut self,
        pid: u32,
        prefix: &str,
        last_child: Option<bool>,
    ) -> Result<(), cash_core::Error> {
        if !self.visited.insert(pid) {
            return Ok(());
        }
        let Some(process) = self.processes.get(&pid) else {
            return Ok(());
        };

        let branch = match last_child {
            None => "",
            Some(true) => "└─",
            Some(false) => "├─",
        };
        if self.show_pids {
            writeln!(self.out, "{prefix}{branch}{}({pid})", process.name)?;
        } else {
            writeln!(self.out, "{prefix}{branch}{}", process.name)?;
        }

        let child_prefix = match last_child {
            None => String::new(),
            Some(true) => std::format!("{prefix}  "),
            Some(false) => std::format!("{prefix}│ "),
        };
        if let Some(children) = self.children.get(&pid) {
            for (index, child) in children.iter().enumerate() {
                self.node(*child, &child_prefix, Some(index + 1 == children.len()))?;
            }
        }
        Ok(())
    }
}
