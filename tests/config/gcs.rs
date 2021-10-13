use std::path::PathBuf;

use gcs_sync::{
    oauth2::token::AuthorizedUserCredentials,
    storage::{credentials, Object},
};

#[allow(dead_code)] //remove this when this issue will be fixed: https://github.com/rust-lang/rust/issues/46379
pub struct GcsTestConfig {
    pub bucket: String,
    pub prefix: PathBuf,
    list_prefix: String,
    pub token: AuthorizedUserCredentials,
}

#[allow(dead_code)] //remove this when this issue will be fixed: https://github.com/rust-lang/rust/issues/46379
impl GcsTestConfig {
    fn of_env_var(key: &str) -> String {
        std::env::var(key)
            .map_err(|x| format!("missing {} env var: {:?}", key, x))
            .unwrap()
    }

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
            let mut prefix = to_path_buf(Self::of_env_var("PREFIX").as_str());
            let uuid = uuid::Uuid::new_v4().to_hyphenated().to_string();
            prefix.push(uuid);
            prefix
        };

        let auc = credentials::authorizeduser::default().await.unwrap();
        Self {
            bucket: Self::of_env_var("BUCKET"),
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
}
