use std::path::{Path, PathBuf};

pub mod token;

#[derive(Debug)]
pub enum Error {
    DeserializationError {
        expected_type: String,
        error: serde_json::Error,
    },
    EnvVarError {
        key: String,
        error: std::env::VarError,
    },
    IoError {
        path: PathBuf,
        message: String,
        error: std::io::Error,
    },
    HttpError(reqwest::Error),
    JWTError(jsonwebtoken::errors::Error),
    MissingScope,
    UnexpectedApiResponse {
        expected_type: String,
        json: serde_json::Value,
    },
}

impl Error {
    pub fn unexpected_api_response<T>(json: serde_json::Value) -> Error {
        let expected_type = std::any::type_name::<T>().to_owned();
        Error::UnexpectedApiResponse {
            expected_type,
            json,
        }
    }

    pub fn io_error<T, U>(message: T, path: U, error: std::io::Error) -> Error
    where
        T: AsRef<str>,
        U: AsRef<Path>,
    {
        let message = message.as_ref().to_owned();
        let path = path.as_ref().to_owned();
        Error::IoError {
            message,
            error,
            path,
        }
    }

    pub fn env_var_error(key: &str, error: std::env::VarError) -> Error {
        Error::EnvVarError {
            key: key.to_owned(),
            error,
        }
    }

    pub fn deserialization_error<T>(error: serde_json::Error) -> Error {
        let expected_type = std::any::type_name::<T>().to_owned();
        Error::DeserializationError {
            expected_type,
            error,
        }
    }
}

type TokenResult<T> = std::result::Result<T, Error>;
