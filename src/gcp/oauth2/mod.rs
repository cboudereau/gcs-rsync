pub mod token;

#[derive(Debug)]
pub enum Error {
    DeserializationError(serde_json::Error),
    EnvVarError(std::env::VarError),
    IoError(std::io::Error),
    HttpError(reqwest::Error),
    JWTError(jsonwebtoken::errors::Error),
    MissingScope,
    UnexpectedApiResponse(serde_json::Value),
}

type TokenResult<T> = std::result::Result<T, Error>;
