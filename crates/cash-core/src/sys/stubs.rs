#![expect(clippy::missing_const_for_fn)]
#![allow(clippy::needless_pass_by_ref_mut)]
#![allow(clippy::needless_pass_by_value)]
#![expect(clippy::unnecessary_wraps)]
// These stubs stand in for POSIX facilities Windows lacks (descriptor passing, resource
// limits, async pipes). They keep the signatures the shell code was written against --
// including `async` on functions a real implementation would await in. A stub body that
// never awaits is therefore that contract being honored, not a defect.
#![allow(clippy::unused_async)]
#![allow(
    clippy::unused_async_trait_impl,
    reason = "stubs keep the async signatures the shell code calls"
)]
#![allow(clippy::unused_self)]

pub mod async_pipe;
pub mod commands;
pub mod fd;
pub mod fs;
pub mod terminal;
