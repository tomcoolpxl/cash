use std::sync::Arc;

use tokio::sync::Mutex;

/// A reference-counted, thread-safe reference to a `cash_core::Shell`.
#[expect(type_alias_bounds)]
pub type ShellRef<SE: cash_core::ShellExtensions = cash_core::extensions::DefaultShellExtensions> =
    Arc<Mutex<cash_core::Shell<SE>>>;
