/// Placeholder
use semver::Version;
use std::{
    io,
    path::{Path, PathBuf},
};
use thiserror::Error;

/// Placeholder
pub type Result<T> = std::result::Result<T, SolcError>;

/// Various error types
#[derive(Debug, Error)]
pub enum SolcError {
    /// Internal solc error
    #[error("Solc Error: {0}")]
    SolcError(String),
    /// Placeholder
    #[error("Missing pragma from solidity file")]
    PragmaNotFound,
    /// Placeholder
    #[error("Could not find solc version locally or upstream")]
    VersionNotFound,
    /// Placeholder
    #[error("Checksum mismatch for {file}: expected {expected} found {detected} for {version}")]
    ChecksumMismatch {
        /// Placeholder
        version: Version,
        /// Placeholder
        expected: String,
        /// Placeholder
        detected: String,
        /// Placeholder
        file: PathBuf,
    },
    /// Placeholder
    #[error("Checksum not found for {version}")]
    ChecksumNotFound {
        /// Placeholder
        version: Version,
    },
    /// Placeholder
    #[error(transparent)]
    SemverError(#[from] semver::Error),
    /// Deserialization error
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
    /// Filesystem IO error
    #[error(transparent)]
    Io(#[from] SolcIoError),
    /// Placeholder
    #[error("File could not be resolved due to broken symlink: {0}.")]
    ResolveBadSymlink(SolcIoError),
    /// Failed to resolve a file
    #[error("Failed to resolve file: {0}.\n Check configured remappings.")]
    Resolve(SolcIoError),
    /// Placeholder
    #[error("File cannot be resolved due to mismatch of file name case: {error}.\n Found existing file: {existing_file:?}\n Please check the case of the import.")]
    ResolveCaseSensitiveFileName {
        /// Placeholder
        error: SolcIoError,
        /// Placeholder
        existing_file: PathBuf,
    },
    /// Placeholder
    #[error(
        r#"{0}.
    --> {1:?}
        {2:?}"#
    )]
    FailedResolveImport(Box<SolcError>, PathBuf, PathBuf),
    #[cfg(all(feature = "svm-solc", not(target_arch = "wasm32")))]
    #[error(transparent)]
    SvmError(#[from] svm::SolcVmError),
    /// Placeholder
    #[error("No contracts found at \"{0}\"")]
    NoContracts(String),
    /// Placeholder
    #[error(transparent)]
    PatternError(#[from] glob::PatternError),
    /// General purpose message
    #[error("{0}")]
    Message(String),
    /// Placeholder
    #[error("No artifact found for `{}:{}`", .0.display(), .1)]
    ArtifactNotFound(PathBuf, String),

    #[cfg(feature = "project-util")]
    #[error(transparent)]
    FsExtra(#[from] fs_extra::error::Error),
}

impl SolcError {
    pub(crate) fn io(err: io::Error, path: impl Into<PathBuf>) -> Self {
        SolcIoError::new(err, path).into()
    }
    pub(crate) fn solc(msg: impl Into<String>) -> Self {
        SolcError::SolcError(msg.into())
    }
    /// Placeholder
    pub fn msg(msg: impl Into<String>) -> Self {
        SolcError::Message(msg.into())
    }
}

macro_rules! _format_err {
    ($($tt:tt)*) => {
        $crate::error::SolcError::msg(format!($($tt)*))
    };
}
#[allow(unused)]
pub(crate) use _format_err as format_err;

macro_rules! _bail {
    ($($tt:tt)*) => { return Err($crate::error::format_err!($($tt)*)) };
}
#[allow(unused)]
pub(crate) use _bail as bail;

#[derive(Debug, Error)]
#[error("\"{}\": {io}", self.path.display())]

/// Placeholder
pub struct SolcIoError {
    io: io::Error,
    /// Placeholder
    path: PathBuf,
}

impl SolcIoError {
    /// Placeholder
    pub fn new(io: io::Error, path: impl Into<PathBuf>) -> Self {
        Self {
            io,
            path: path.into(),
        }
    }

    /// The path at which the error occurred
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The underlying `io::Error`
    pub fn source(&self) -> &io::Error {
        &self.io
    }
}

impl From<SolcIoError> for io::Error {
    fn from(err: SolcIoError) -> Self {
        err.io
    }
}
