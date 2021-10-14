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

    pub fn io_error(message: String, error: std::io::Error) -> Error {
        Error::IoError { message, error }
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
