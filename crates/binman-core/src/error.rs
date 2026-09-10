use std::fmt::Display;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),

    /// A file is not in the shape its name promised.
    #[error("{what}: {message}")]
    Parse { what: String, message: String },

    #[error("{0}")]
    Config(String),

    /// The request cannot be sent as it stands — a URL that does not parse, a
    /// header name with a space in it, a file to upload that is not there.
    /// Nothing left the machine.
    #[error("{0}")]
    Invalid(String),

    /// The server could not be reached, or stopped answering part-way.
    #[error("{0}")]
    Http(String),

    /// The token endpoint said no, or answered with something that is not a
    /// token.
    #[error("OAuth2: {0}")]
    OAuth(String),

    /// Called off. An outcome rather than a fault: nobody is waiting for the
    /// answer any more.
    #[error("cancelled")]
    Cancelled,
}

impl Error {
    pub fn parse(what: impl Display, message: impl Display) -> Error {
        Error::Parse {
            what: what.to_string(),
            message: message.to_string(),
        }
    }
}
