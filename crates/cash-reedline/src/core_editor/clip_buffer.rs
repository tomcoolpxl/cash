use crate::Granularity;

/// Defines an interface to interact with a Clipboard for cut and paste.
///
/// Mutable reference requirements are stricter than always necessary, but the currently used system clipboard API demands them for exclusive access.
pub trait Clipboard: Send {
    fn set(&mut self, content: &str, mode: Granularity);

    fn get(&mut self) -> (String, Granularity);

    #[allow(dead_code)]
    fn clear(&mut self) {
        self.set("", Granularity::CharWise);
    }

    #[allow(dead_code)]
    fn len(&mut self) -> usize {
        self.get().0.len()
    }
}

/// Simple buffer that provides a clipboard only usable within the application/library.
#[derive(Default)]
pub struct LocalClipboard {
    content: String,
    mode: Granularity,
}

impl LocalClipboard {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Clipboard for LocalClipboard {
    fn set(&mut self, content: &str, mode: Granularity) {
        content.clone_into(&mut self.content);
        self.mode = mode;
    }

    fn get(&mut self) -> (String, Granularity) {
        (self.content.clone(), self.mode)
    }
}

/// Creates a local clipboard
pub fn get_local_clipboard() -> Box<dyn Clipboard> {
    Box::new(LocalClipboard::new())
}

#[cfg(test)]
mod tests {
    use super::get_local_clipboard;
    use crate::Granularity;
    #[test]
    fn reads_back_local() {
        let mut cb = get_local_clipboard();
        // If the system clipboard is used we want to persist it for the user
        let previous_state = cb.get().0;

        // Actual test
        cb.set("test", Granularity::CharWise);
        assert_eq!(cb.len(), 4);
        assert_eq!(cb.get().0, "test".to_owned());
        cb.clear();
        assert_eq!(cb.get().0, String::new());

        // Restore!

        cb.set(&previous_state, Granularity::CharWise);
    }
}
