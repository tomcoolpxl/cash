/// A password used for password protected, encrypted files.
///
/// Use [`Password::empty()`] to create an empty password when no
/// password is used.
///
/// You can convert strings easily into password using the Into/From traits:
///
/// ```rust
/// use cash_archive::sevenz::Password;
///
/// let password: Password = "a password string".into();
/// ```
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Password(Vec<u8>);

impl Password {
    /// Creates a new [`Password`] from the given password string.
    ///
    /// Internally a password string is encoded as UTF-16.
    pub fn new(password: &str) -> Self {
        Self::from(password)
    }

    /// Creates a new [`Password`] from the given raw bytes.
    pub fn from_raw(bytes: &[u8]) -> Self {
        Self(bytes.to_vec())
    }

    /// Creates an empty password.
    pub const fn empty() -> Self {
        Self(Vec::new())
    }

    /// Returns the byte representation of the password.
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    /// Returns `true` if the password is empty.
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl AsRef<[u8]> for Password {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<&str> for Password {
    fn from(s: &str) -> Self {
        Self(s.encode_utf16().flat_map(u16::to_le_bytes).collect())
    }
}
