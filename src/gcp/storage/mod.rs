mod client;
mod object;
mod resources;

pub use object::ObjectClient;
pub use resources::object::{
    Bucket, Metadata, Object, ObjectMetadata, ObjectsListRequest, PartialObject,
};

pub mod credentials {

    pub mod serviceaccount {

        use crate::gcp::oauth2::token::ServiceAccountCredentials;

        pub async fn default(
            scope: &str,
        ) -> super::super::StorageResult<ServiceAccountCredentials> {
            ServiceAccountCredentials::default()
                .await
                .map(|x| x.with_scope(scope))
                .map_err(super::super::Error::GcsTokenError)
        }

        pub fn from_str(
            str: &str,
            scope: &str,
        ) -> super::super::StorageResult<ServiceAccountCredentials> {
            ServiceAccountCredentials::from(str)
                .map(|x| x.with_scope(scope))
                .map_err(super::super::Error::GcsTokenError)
        }

        pub async fn from_file<T>(
            file_path: T,
            scope: &str,
        ) -> super::super::StorageResult<ServiceAccountCredentials>
        where
            T: AsRef<std::path::Path>,
        {
            ServiceAccountCredentials::from_file(file_path)
                .await
                .map(|x| x.with_scope(scope))
                .map_err(super::super::Error::GcsTokenError)
        }
    }

    pub mod authorizeduser {

        use crate::gcp::oauth2::token::AuthorizedUserCredentials;

        pub async fn default() -> super::super::StorageResult<AuthorizedUserCredentials> {
            AuthorizedUserCredentials::default()
                .await
                .map_err(super::super::Error::GcsTokenError)
        }

        pub fn from_str(str: &str) -> super::super::StorageResult<AuthorizedUserCredentials> {
            AuthorizedUserCredentials::from(str).map_err(super::super::Error::GcsTokenError)
        }

        pub async fn from_file<T>(
            file_path: T,
        ) -> super::super::StorageResult<AuthorizedUserCredentials>
        where
            T: AsRef<std::path::Path>,
        {
            AuthorizedUserCredentials::from_file(file_path)
                .await
                .map_err(super::super::Error::GcsTokenError)
        }
    }

    pub mod metadata {

        use crate::oauth2::token::GoogleMetadataServerCredentials;

        pub fn default() -> super::super::StorageResult<GoogleMetadataServerCredentials> {
            GoogleMetadataServerCredentials::new().map_err(super::super::Error::GcsTokenError)
        }
        pub fn with_scope(
            scope: &str,
        ) -> super::super::StorageResult<GoogleMetadataServerCredentials> {
            GoogleMetadataServerCredentials::new()
                .map(|x| x.with_scope(scope))
                .map_err(super::super::Error::GcsTokenError)
        }
    }
}

#[derive(Debug)]
pub enum Error {
    GcsTokenError(super::oauth2::Error),
    GcsHttpJsonRequestError(reqwest::Error),
    GcsHttpJsonResponseError(reqwest::Error),
    GcsHttpBytesStreamError(reqwest::Error),
    GcsHttpGetAsStreamError(reqwest::Error),
    GcsHttpPostMultipartError(reqwest::Error),
    GcsHttpPostError(reqwest::Error),
    GcsHttpDeleteError(reqwest::Error),
    GcsHttpNoTextError(reqwest::Error),
    GcsUnexpectedResponse {
        url: String,
        value: String,
    },
    GcsUnexpectedJson {
        url: String,
        expected_type: String,
        json: serde_json::Value,
    },
    GcsPartialResponseError(String),
    GcsInvalidUrl {
        url: String,
        message: String,
    },
    GcsInvalidObjectName,
    GcsResourceNotFound {
        url: String,
    },
    InvalidMetadata {
        expected_type: String,
        error: serde_json::Error,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
impl std::error::Error for Error {}

impl Error {
    fn gcs_invalid_metadata<T>(error: serde_json::Error) -> Self {
        Self::InvalidMetadata {
            expected_type: std::any::type_name::<T>().to_owned(),
            error,
        }
    }
    fn gcs_unexpected_response_error<T, U>(url: T, value: U) -> Self
    where
        T: AsRef<str>,
        U: AsRef<str>,
    {
        Self::GcsUnexpectedResponse {
            url: url.as_ref().to_owned(),
            value: value.as_ref().to_owned(),
        }
    }

    fn gcs_unexpected_json<T>(url: &str, json: serde_json::Value) -> Self {
        let expected_type = std::any::type_name::<T>().to_owned();
        Self::GcsUnexpectedJson {
            url: url.to_owned(),
            expected_type,
            json,
        }
    }
}

pub type StorageResult<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use crate::storage::Error;
    #[test]
    fn test_error_display() {
        let e = Error::gcs_unexpected_response_error("url", "value");
        let actual = format!("{}", e);

        assert_eq!(
            "GcsUnexpectedResponse { url: \"url\", value: \"value\" }",
            actual
        );
    }
}
