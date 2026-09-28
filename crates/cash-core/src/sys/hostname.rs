pub(crate) fn get() -> std::io::Result<std::ffi::OsString> {
    // cash: Windows has two names for one machine. The DNS hostname API — which the
    // `hostname` crate below uses, as do .NET and uutils — answers `desktop-tomc`, while
    // `%COMPUTERNAME%`, the domain half of `id -un` and the rest of the system answer
    // `DESKTOP-TOMC`. Reporting both leaves `[ "$(hostname)" = "$COMPUTERNAME" ]` false
    // inside one shell, so cash reports the spelling everything else agrees on and falls
    // back to the portable lookup only if Windows will not answer.
    if let Some(name) = cash_win32::process::computer_name() {
        return Ok(std::ffi::OsString::from(name));
    }

    hostname::get()
}
