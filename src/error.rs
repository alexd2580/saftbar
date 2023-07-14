use std::fmt::Display;

#[derive(Debug)]
pub enum Xcb {
    Regular(xcb::Error),
    Protocol(xcb::ProtocolError),
    Conn(xcb::ConnError),
}

#[derive(Debug)]
pub enum Error {
    Local(String),
    Xcb(Xcb),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Local(message) => write!(f, "{message}"),
            Error::Xcb(err) => write!(f, "{err:#?}"),
        }
    }
}

impl From<xcb::Error> for Error {
    fn from(value: xcb::Error) -> Self {
        Self::Xcb(Xcb::Regular(value))
    }
}

impl From<xcb::ProtocolError> for Error {
    fn from(value: xcb::ProtocolError) -> Self {
        Self::Xcb(Xcb::Protocol(value))
    }
}

impl From<xcb::ConnError> for Error {
    fn from(value: xcb::ConnError) -> Self {
        Self::Xcb(Xcb::Conn(value))
    }
}
