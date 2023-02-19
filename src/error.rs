use serde::de;
use std::fmt::{self, Display};

#[cfg(feature = "from_str")]
use serde_urlencoded::de::Error as UrlEncodedError;

#[derive(Debug)]
pub enum Error {
    TrailingValues,
    ForbiddenNestedMap,
    ForbiddenNestedSequence,
    SequenceNotConsumed,
    MissingKey,
    MissingValues,
    MissingValue,
    ForbiddenTopLevelOption,
    ExpectedUnitVariant,
    RemoveKeyFailed(String),
    Message(String),
    #[cfg(feature = "from_str")]
    UrlEncoded(UrlEncodedError),
}

impl de::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::Message(msg.to_string())
    }
}

#[cfg(feature = "from_str")]
impl From<UrlEncodedError> for Error {
    fn from(e: UrlEncodedError) -> Self {
        Self::UrlEncoded(e)
    }
}

impl Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Message(msg) => formatter.write_str(msg),
            _ => write!(formatter, "{:?}", self),
        }
    }
}

impl std::error::Error for Error {}
