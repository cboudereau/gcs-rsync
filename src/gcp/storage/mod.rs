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
    GcsUnexpectedResponse(serde_json::Value),
    GcsPartialResponseError(String),
    GcsResourceNotFound,
}

pub type StorageResult<T> = std::result::Result<T, Error>;
