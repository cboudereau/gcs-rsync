use std::path::PathBuf;

use tokio::{fs::File, io::AsyncReadExt};

pub struct FsTestConfig {
    base_path: PathBuf,
}

#[allow(dead_code)] //remove this when this issue will be fixed: https://github.com/rust-lang/rust/issues/46379
impl FsTestConfig {
    pub fn new() -> Self {
        let base_path = {
            let uuid = uuid::Uuid::new_v4().hyphenated().to_string();
            let mut tmp = std::env::temp_dir();
            tmp.push("rsync_integration_tests");
            tmp.push(uuid);
            tmp
        };
        Self { base_path }
    }

    pub fn base_path(&self) -> PathBuf {
        self.base_path.to_owned()
    }

    pub fn file_path(&self, file_name: &str) -> PathBuf {
        let mut p = self.base_path.clone();
        let file_name = file_name.strip_prefix('/').unwrap_or(file_name);
        p.push(file_name);
        p
    }

    pub async fn read_to_string(&self, file_name: &str) -> String {
        let mut path = self.base_path();
        path.push(file_name);
        let mut f = File::open(path).await.unwrap();
        let mut buffer = String::new();

        f.read_to_string(&mut buffer).await.unwrap();
        buffer
    }
}
