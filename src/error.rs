//! Defines the error type for the hypervisor.

use core::fmt;
use core::error::Error as CoreError;
use core::result::Result as CoreResult;
use alloc::boxed::Box;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorKind {
    Library,
    InvalidParam,
    NotFound,
    AlreadyExists,
}

type DynError = dyn CoreError + Send + Sync;

#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    inner: Option<Box<DynError>>,
}

pub type Result<T> = CoreResult<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> CoreResult<(), fmt::Error> {
        fmt::Debug::fmt(self, f)
    }
}

#[allow(dead_code)]
impl Error {
    pub fn new(kind: ErrorKind, inner: Box<DynError>) -> Self {
        Self {
            kind,
            inner: Some(inner),
        }
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    pub fn into_inner(self) -> Option<Box<DynError>> {
        self.inner
    }
}

impl ErrorKind {
    pub fn wrap(self, inner: Box<DynError>) -> Error {
        Error::new(self, inner)
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Self {
        Self { kind, inner: None }
    }
}

impl<T> From<ErrorKind> for Result<T> {
    fn from(val: ErrorKind) -> Self {
        Err(val.into())
    }
}

impl<T: CoreError + Send + Sync + 'static> From<T> for Error {
    fn from(e: T) -> Self {
        ErrorKind::Library.wrap(Box::new(e))
    }
}
