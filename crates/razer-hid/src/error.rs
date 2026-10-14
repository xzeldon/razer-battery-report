//! The error type of the crate.

/// Everything that can go wrong while talking to a mouse.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The mouse did not answer after all retries, usually because it is asleep.
    /// Callers should treat this as "no reading right now", not as a failure.
    #[error("the mouse did not respond (it may be asleep)")]
    Timeout,
    /// The mouse understood the request and said it does not support the command.
    #[error("the command is not supported by this mouse")]
    NotSupported,
    /// The mouse understood the request and reported that it failed.
    #[error("the mouse reported a failure")]
    Failure,
    /// The answer did not match the request, had a bad checksum or the wrong length.
    #[error("invalid response: {0}")]
    InvalidResponse(&'static str),
    /// The operating system or hidapi failed to talk to the device.
    #[error("hid transport error: {0}")]
    Transport(#[from] hidapi::HidError),
}

/// Shorthand for a result with this crate's [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
