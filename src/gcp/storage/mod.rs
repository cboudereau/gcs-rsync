mod client;
mod object;
mod resources;

pub use object::ObjectClient;
pub use resources::object::{Bucket, Object, ObjectsListRequest, PartialObject};

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
            Ok(AuthorizedUserCredentials::default()
                .await
                .map_err(super::super::Error::GcsTokenError)?)
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
            Ok(AuthorizedUserCredentials::from_file(file_path)
                .await
                .map_err(super::super::Error::GcsTokenError)?)
        }
    }
}

#[derive(Debug)]
pub enum Error {
    GcsTokenError(super::oauth2::Error),
    GcsHttpError(reqwest::Error),
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
}

impl Error {
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
