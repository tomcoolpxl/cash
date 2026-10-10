// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! External sort: sort large inputs that may not fit in memory.
//!
//! A multi-threaded chunked approach with temporary files.

mod threaded;
pub use threaded::ext_sort;
