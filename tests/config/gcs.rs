use gcs_rsync::{oauth2::token::ServiceAccountCredentials, storage::Object};

pub struct GcsTestConfig {
    bucket: String,
    prefix: String,
    list_prefix: String,
    token: ServiceAccountCredentials,
}

#[allow(dead_code)] //remove this when this issue will be fixed: https://github.com/rust-lang/rust/issues/46379
impl GcsTestConfig {
    pub async fn from_env() -> Self {
        fn to_path(path: &str) -> String {
            let path = path.strip_prefix('/').unwrap_or(path);
            if path.ends_with('/') {
                path.to_owned()
            } else {
                format!("{}/", path)
            }
        }

        let prefix = {
            let prefix = to_path(env!("TEST_PREFIX"));
            let uuid = uuid::Uuid::new_v4().hyphenated().to_string();
            format!("{prefix}{uuid}")
        };
        let path = env!("TEST_SERVICE_ACCOUNT");
        let sac = ServiceAccountCredentials::from_file(path)
            .await
            .unwrap()
            .with_scope("https://www.googleapis.com/auth/devstorage.full_control");
        Self {
            bucket: env!("TEST_BUCKET").to_owned(),
            prefix: prefix.to_owned(),
            list_prefix: prefix.to_owned(),
            token: sac,
        }
    }

    pub fn object(&self, name: &str) -> Object {
        let path = self.prefix.clone();
        let name = format!("{path}/{name}");
        Object {
            bucket: self.bucket.to_owned(),
            name,
        }
    }

    pub fn list_prefix(&self) -> String {
        self.list_prefix.to_owned()
    }

    pub fn bucket(&self) -> String {
        self.bucket.to_owned()
    }

    pub fn prefix_as_folder(&self) -> String {
        let prefix = self.prefix.as_str();
        format!("{prefix}/")
    }

    pub fn prefix(&self) -> String {
        let prefix = self.prefix.as_str();
        prefix.strip_suffix('/').unwrap_or(prefix).to_owned()
    }

    pub fn token(self) -> ServiceAccountCredentials {
        self.token
    }
}
