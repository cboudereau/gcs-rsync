use std::path::PathBuf;

use gcs_rsync::{
    oauth2::token::AuthorizedUserCredentials,
    storage::{credentials, Object},
};

pub struct GcsTestConfig {
    bucket: String,
    prefix: PathBuf,
    list_prefix: String,
    token: AuthorizedUserCredentials,
}

#[allow(dead_code)] //remove this when this issue will be fixed: https://github.com/rust-lang/rust/issues/46379
impl GcsTestConfig {
    pub async fn from_env() -> Self {
        fn to_path_buf(path: &str) -> PathBuf {
            let path = path.strip_prefix('/').unwrap_or(path);
            let path = if path.ends_with('/') {
                path.to_owned()
            } else {
                format!("{}/", path)
            };

            PathBuf::from(path)
        }

        let prefix = {
            let mut prefix = to_path_buf(env!("TEST_PREFIX"));
            let uuid = uuid::Uuid::new_v4().to_hyphenated().to_string();
            prefix.push(uuid);
            prefix
        };

        let auc = credentials::authorizeduser::default().await.unwrap();
        Self {
            bucket: env!("TEST_BUCKET").to_owned(),
            prefix: prefix.to_owned(),
            list_prefix: prefix.to_string_lossy().to_string(),
            token: auc,
        }
    }

    pub fn object(&self, name: &str) -> Object {
        let mut path = self.prefix.clone();
        path.push(name);
        Object {
            bucket: self.bucket.to_owned(),
            name: path.to_string_lossy().to_string(),
        }
    }

    pub fn list_prefix(&self) -> String {
        self.list_prefix.to_owned()
    }

    pub fn bucket(&self) -> String {
        self.bucket.to_owned()
    }

    pub fn prefix(&self) -> PathBuf {
        self.prefix.to_owned()
    }

    pub fn token(self) -> AuthorizedUserCredentials {
        self.token
    }
}
