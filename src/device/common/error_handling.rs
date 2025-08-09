// Common error types and result aliases for device modules.

use std::fmt;
use std::io;

#[derive(Debug)]
#[allow(dead_code)]
pub enum DeviceError {
    Io(io::Error),
    Timeout(String),
    CommandFailed {
        command: String,
        code: Option<i32>,
        stderr: String,
    },
    ParseError(String),
    Other(String),
}

pub type DeviceResult<T> = Result<T, DeviceError>;

impl fmt::Display for DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceError::Io(e) => write!(f, "IO error: {e}"),
            DeviceError::Timeout(msg) => write!(f, "Timeout: {msg}"),
            DeviceError::CommandFailed {
                command,
                code,
                stderr,
            } => {
                write!(
                    f,
                    "Command failed: '{command}' (code: {code:?}) stderr: {stderr}"
                )
            }
            DeviceError::ParseError(msg) => write!(f, "Parse error: {msg}"),
            DeviceError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for DeviceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DeviceError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for DeviceError {
    fn from(value: io::Error) -> Self {
        DeviceError::Io(value)
    }
}
