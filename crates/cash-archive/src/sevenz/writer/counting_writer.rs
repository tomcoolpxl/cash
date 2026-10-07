use std::{
    cell::Cell,
    io::{self, Write},
    rc::Rc,
};

/// Counts what a coder writes into the next one out: that coder's output size, which the
/// header stores for each coder.
pub(crate) struct CountingWriter<W> {
    inner: W,
    counting: Rc<Cell<u64>>,
}

impl<W> CountingWriter<W> {
    pub(crate) fn new(inner: W) -> Self {
        Self {
            inner,
            counting: Rc::new(Cell::new(0)),
        }
    }

    pub(crate) fn counting(&self) -> Rc<Cell<u64>> {
        Rc::clone(&self.counting)
    }

    pub(crate) fn into_inner(self) -> W {
        self.inner
    }
}

impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let len = self.inner.write(buf)?;
        self.counting.set(self.counting.get() + len as u64);
        Ok(len)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
