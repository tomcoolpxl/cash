use std::io;
use std::time::Duration;

use crossterm_winapi::Handle;
use winapi::{
    shared::winerror::WAIT_TIMEOUT,
    um::{
        synchapi::WaitForMultipleObjects,
        winbase::{INFINITE, WAIT_ABANDONED_0, WAIT_FAILED, WAIT_OBJECT_0},
    },
};

#[derive(Debug)]
pub(crate) struct WinApiPoll {}

impl WinApiPoll {
    pub(crate) fn new() -> WinApiPoll {
        WinApiPoll {}
    }
}

impl WinApiPoll {
    pub fn poll(&mut self, timeout: Option<Duration>) -> std::io::Result<Option<bool>> {
        let dw_millis = if let Some(duration) = timeout {
            duration.as_millis() as u32
        } else {
            INFINITE
        };

        let console_handle = Handle::current_in_handle()?;

        let handles = &[*console_handle];

        // SAFETY: the slice holds one open console input handle, and its length is given.
        let output =
            unsafe { WaitForMultipleObjects(handles.len() as u32, handles.as_ptr(), 0, dw_millis) };

        match output {
            output if output == WAIT_OBJECT_0 => {
                // input handle triggered
                Ok(Some(true))
            }
            WAIT_TIMEOUT | WAIT_ABANDONED_0 => {
                // timeout elapsed
                Ok(None)
            }
            WAIT_FAILED => Err(io::Error::last_os_error()),
            _ => Err(io::Error::new(
                io::ErrorKind::Other,
                "WaitForMultipleObjects returned unexpected result.",
            )),
        }
    }
}
