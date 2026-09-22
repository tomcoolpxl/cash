//! `hostname` — **D48**, for the reason **§4 #20** exists.
//!
//! Windows has two names for one machine, and cash was using both. The DNS hostname API —
//! which uutils' `hostname`, the `hostname` crate and .NET's `Dns.GetHostName` all call —
//! answers `desktop-tomc`, while `%COMPUTERNAME%`, the domain half of `id -un` and every
//! Windows tool answer `DESKTOP-TOMC`. So inside one shell:
//!
//! ```bash
//! [ "$(hostname)" = "$COMPUTERNAME" ]   # false
//! ```
//!
//! which is the same shape as the `$UID` disagreeing with `id -u` that §4 #20 records:
//! not a missing feature, a shell contradicting itself about who and where it is.
//!
//! cash answers with the spelling the rest of the system agrees on, here and in
//! `$HOSTNAME`.

use std::io::Write;

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// Show the machine's name.
#[derive(Parser)]
pub(crate) struct HostnameCommand {
    /// Show the short name, without any domain.
    #[arg(short = 's', long = "short")]
    short: bool,

    /// Show the fully qualified domain name.
    #[arg(short = 'f', long = "fqdn", conflicts_with = "short")]
    fqdn: bool,

    /// Show the DNS domain the machine is in.
    #[arg(short = 'd', long = "domain", conflicts_with_all = ["short", "fqdn"])]
    domain: bool,
}

impl builtins::Command for HostnameCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let Some(name) = cash_win32::process::computer_name() else {
            writeln!(
                context.stderr(),
                "{}: cannot determine the machine name",
                context.command_name
            )?;
            return Ok(ExecutionResult::general_error());
        };

        // The DNS suffix, when the machine has one — a domain-joined machine does, a
        // workgroup machine usually does not.
        let suffix = cash_win32::process::dns_domain();

        let answer = if self.domain {
            suffix.unwrap_or_default()
        } else if self.fqdn {
            match suffix {
                Some(suffix) if !suffix.is_empty() => std::format!("{name}.{suffix}"),
                _ => name,
            }
        } else {
            // `-s` and the bare form agree here: the NetBIOS name never carries a domain.
            name
        };

        writeln!(context.stdout(), "{answer}")?;

        Ok(ExecutionResult::success())
    }
}
