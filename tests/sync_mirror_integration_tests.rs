use std::path::{Path, PathBuf};

use futures::{StreamExt, TryStreamExt};
use gcs_rsync::{
    storage::credentials::authorizeduser,
    sync::{
        DefaultRSync, DefaultSource, RMirrorStatus, RSync, RSyncStatus, ReaderWriter, RelativePath,
    },
};
use tokio::io::AsyncWriteExt;

struct FsTestConfig {
    base_path: PathBuf,
}

mod config;
use config::gcs::GcsTestConfig;

impl FsTestConfig {
    fn new() -> Self {
        let base_path = {
            let uuid = uuid::Uuid::new_v4().to_hyphenated().to_string();
            let mut tmp = std::env::temp_dir();
            tmp.push("rsync_integration_tests");
            tmp.push(uuid);
            tmp
        };
        Self { base_path }
    }

    fn file_path(&self, file_name: &str) -> PathBuf {
        let mut p = self.base_path.clone();
        let file_name = file_name.strip_prefix('/').unwrap_or(file_name);
        p.push(file_name);
        p
    }
}

impl Drop for FsTestConfig {
    fn drop(&mut self) {
        let path = self.base_path.as_path();
        std::fs::remove_dir_all(path).unwrap();
    }
}

async fn write_to_file(path: &Path, content: &str) {
    assert_create_file(path, content).await;
}

async fn assert_create_file(path: &Path, content: &str) {
    let path = PathBuf::from(path);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.unwrap();
    }
    let mut file = tokio::fs::File::create(path).await.unwrap();
    file.write_all(content.as_bytes()).await.unwrap();
}

async fn delete_file(path: &Path) {
    tokio::fs::remove_file(path).await.unwrap()
}

async fn setup_files(file_names: &[PathBuf], content: &str) {
    futures::stream::iter(file_names)
        .for_each_concurrent(config::default::CONCURRENCY_LEVEL, |x| {
            assert_create_file(x.as_path(), content)
        })
        .await;
}

async fn delete_files(file_names: &[PathBuf]) {
    futures::stream::iter(file_names)
        .for_each_concurrent(config::default::CONCURRENCY_LEVEL, |x| {
            delete_file(x.as_path())
        })
        .await;
}

async fn sync(fs_client: &DefaultRSync) -> Vec<RSyncStatus> {
    let mut actual = fs_client
        .sync()
        .await
        .try_buffer_unordered(config::default::CONCURRENCY_LEVEL)
        .try_collect::<Vec<_>>()
        .await
        .unwrap();
    actual.sort();
    actual
}

async fn mirror(fs_client: &DefaultRSync) -> Vec<RMirrorStatus> {
    let mut actual = fs_client
        .mirror()
        .await
        .try_buffer_unordered(config::default::CONCURRENCY_LEVEL)
        .try_collect::<Vec<_>>()
        .await
        .unwrap();
    actual.sort();
    actual
}

fn created(path: &str) -> RSyncStatus {
    RSyncStatus::Created(RelativePath::new(path).unwrap())
}

fn updated(path: &str) -> RSyncStatus {
    RSyncStatus::Updated(RelativePath::new(path).unwrap())
}

fn already_sinced(path: &str) -> RSyncStatus {
    RSyncStatus::AlreadySynced(RelativePath::new(path).unwrap())
}

fn deleted(path: &str) -> RMirrorStatus {
    RMirrorStatus::Deleted(RelativePath::new(path).unwrap())
}

fn not_deleted(path: &str) -> RMirrorStatus {
    RMirrorStatus::NotDeleted(RelativePath::new(path).unwrap())
}

fn synced(x: RSyncStatus) -> RMirrorStatus {
    RMirrorStatus::Synced(x)
}

#[tokio::test]
async fn test_fs_to_gcs_sync_and_mirror() {
    let src_t = FsTestConfig::new();

    let file_names = vec![
        "/hello/world/test.txt",
        "test.json",
        "a/long/path/hello_world.toml",
    ]
    .into_iter()
    .map(|x| src_t.file_path(x))
    .collect::<Vec<_>>();

    setup_files(&file_names[..], "Hello World").await;

    async fn generate_gcs(test_config: GcsTestConfig) -> DefaultSource {
        let bucket = test_config.bucket();
        let prefix = test_config.prefix();

        let gcs_dest = DefaultSource::gcs(
            test_config.token(),
            bucket.as_str(),
            prefix.to_str().unwrap(),
        )
        .await
        .unwrap();

        gcs_dest
    }
    let gcs_dst_t = GcsTestConfig::from_env().await;

    let gcs_source_replica = async {
        let token_generator = authorizeduser::default().await.unwrap();
        let bucket = gcs_dst_t.bucket();
        let prefix = gcs_dst_t.prefix();
        ReaderWriter::gcs(token_generator, bucket.as_str(), prefix.to_str().unwrap())
            .await
            .unwrap()
    }
    .await;

    let gcs_dest_replica = generate_gcs(GcsTestConfig::from_env().await).await;

    let gcs_dest = generate_gcs(gcs_dst_t).await;

    let fs_source = DefaultSource::fs(src_t.base_path.as_path());
    let rsync_fs_to_gcs = RSync::new(fs_source, gcs_dest);
    let rsync_gcs_to_gcs_replica = RSync::new(gcs_source_replica, gcs_dest_replica);

    let expected = vec![
        created("a/long/path/hello_world.toml"),
        created("hello/world/test.txt"),
        created("test.json"),
    ];
    assert_eq!(expected, sync(&rsync_fs_to_gcs).await);
    assert_eq!(expected, sync(&rsync_gcs_to_gcs_replica).await);

    let expected = vec![
        already_sinced("a/long/path/hello_world.toml"),
        already_sinced("hello/world/test.txt"),
        already_sinced("test.json"),
    ];
    assert_eq!(expected, sync(&rsync_fs_to_gcs).await);
    assert_eq!(expected, sync(&rsync_gcs_to_gcs_replica).await);

    write_to_file(src_t.file_path("test.json").as_path(), "updated").await;
    let new_file = src_t.file_path("new.json");
    write_to_file(new_file.as_path(), "new file").await;
    let expected = vec![
        created("new.json"),
        updated("test.json"),
        already_sinced("a/long/path/hello_world.toml"),
        already_sinced("hello/world/test.txt"),
    ];
    assert_eq!(expected, sync(&rsync_fs_to_gcs).await);
    assert_eq!(expected, sync(&rsync_gcs_to_gcs_replica).await);

    delete_files(&file_names[..]).await;
    let expected = vec![
        synced(already_sinced("new.json")),
        deleted("a/long/path/hello_world.toml"),
        deleted("hello/world/test.txt"),
        deleted("test.json"),
        not_deleted("new.json"),
    ];
    assert_eq!(expected, mirror(&rsync_fs_to_gcs).await);
    assert_eq!(expected, mirror(&rsync_gcs_to_gcs_replica).await);

    delete_file(new_file.as_path()).await;
    let expected = vec![deleted("new.json")];
    assert_eq!(expected, mirror(&rsync_fs_to_gcs).await);
    assert_eq!(expected, mirror(&rsync_gcs_to_gcs_replica).await);
}
