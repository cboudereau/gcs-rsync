use std::path::{Path, PathBuf};

use futures::{StreamExt, TryStreamExt};
use gcs_rsync::{
    oauth2::token::ServiceAccountCredentials,
    storage::{Object, ObjectClient, StorageResult},
    sync::{RMirrorStatus, RSync, RSyncError, RSyncStatus, ReaderWriter, RelativePath, Source},
};
use tokio::io::AsyncWriteExt;

mod config;
use config::{fs::FsTestConfig, gcs::GcsTestConfig};

async fn get_service_account() -> ServiceAccountCredentials {
    let path = env!("TEST_SERVICE_ACCOUNT");
    ServiceAccountCredentials::from_file(path)
        .await
        .unwrap()
        .with_scope("https://www.googleapis.com/auth/devstorage.full_control")
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

async fn sync(rsync: &RSync) -> Vec<RSyncStatus> {
    let mut actual = rsync
        .sync()
        .await
        .try_buffer_unordered(config::default::CONCURRENCY_LEVEL)
        .try_collect::<Vec<_>>()
        .await
        .unwrap();
    actual.sort();
    actual
}

async fn mirror(fs_client: &RSync) -> Vec<RMirrorStatus> {
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

fn updated(reason: &str, path: &str) -> RSyncStatus {
    RSyncStatus::Updated {
        reason: reason.to_owned(),
        path: RelativePath::new(path).unwrap(),
    }
}

fn already_synced(reason: &str, path: &str) -> RSyncStatus {
    RSyncStatus::AlreadySynced {
        reason: reason.to_owned(),
        path: RelativePath::new(path).unwrap(),
    }
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
async fn test_fs_to_gcs_sync_and_mirror_with_restore_fs_mtime() {
    test_fs_to_gcs_sync_and_mirror_base(true).await;
}

#[tokio::test]
async fn test_fs_to_gcs_sync_and_mirror() {
    test_fs_to_gcs_sync_and_mirror_base(false).await;
}

#[tokio::test]
async fn test_sync_and_mirror_crc32c() {
    async fn assert_delete_ok(object_client: &ObjectClient, object: &Object) {
        let delete_result = object_client.delete(object).await;
        assert!(
            delete_result.is_ok(),
            "unexpected error {:?} for {}",
            delete_result,
            object
        );
    }

    async fn upload_bytes(
        object_client: &ObjectClient,
        object: &Object,
        content: &str,
    ) -> StorageResult<()> {
        let data = bytes::Bytes::copy_from_slice(content.as_bytes());
        let stream = futures::stream::once(futures::future::ok::<bytes::Bytes, String>(data));
        object_client.upload(object, stream).await
    }

    async fn assert_upload_bytes(object_client: &ObjectClient, object: &Object, content: &str) {
        let upload_result = upload_bytes(object_client, object, content).await;
        assert!(
            upload_result.is_ok(),
            "unexpected error got {:?} for {}",
            upload_result,
            object
        );
    }

    let gcs_src_config = GcsTestConfig::from_env().await;
    let src_config_bucket = gcs_src_config.bucket();
    let src_config_prefix = gcs_src_config.prefix_as_folder();
    let gcs_dst = GcsTestConfig::from_env().await;

    let object = gcs_src_config.object("hello.txt");

    let object_client = ObjectClient::new(Box::new(gcs_src_config.token()))
        .await
        .unwrap();
    assert_upload_bytes(&object_client, &object, "hello").await;

    let src = Source::gcs(
        Box::new(get_service_account().await),
        &src_config_bucket,
        src_config_prefix.as_str(),
    )
    .await
    .unwrap();

    let bucket = gcs_dst.bucket();
    let prefix = gcs_dst.prefix_as_folder();
    let dest = Source::gcs(Box::new(gcs_dst.token()), &bucket, prefix.as_str())
        .await
        .unwrap();

    let rsync = RSync::new(src, dest);
    assert_eq!(vec![synced(created("hello.txt"))], mirror(&rsync).await);
    assert_eq!(
        vec![
            synced(already_synced("same crc32c", "hello.txt")),
            not_deleted("hello.txt")
        ],
        mirror(&rsync).await
    );

    assert_delete_ok(&object_client, &object).await;

    assert_eq!(vec![deleted("hello.txt")], mirror(&rsync).await);
}

async fn test_fs_to_gcs_sync_and_mirror_base(set_fs_mtime: bool) {
    async fn generate_gcs(test_config: GcsTestConfig) -> Source {
        let bucket = test_config.bucket();
        let prefix = test_config.prefix_as_folder();

        Source::gcs(
            Box::new(test_config.token()),
            bucket.as_str(),
            prefix.as_str(),
        )
        .await
        .unwrap()
    }

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

    let gcs_dst_t = GcsTestConfig::from_env().await;

    let gcs_source_replica = async {
        let token_generator = Box::new(get_service_account().await);
        let bucket = gcs_dst_t.bucket();
        let prefix = gcs_dst_t.prefix_as_folder();
        ReaderWriter::gcs(token_generator, bucket.as_str(), prefix.as_str())
            .await
            .unwrap()
    }
    .await;

    let gcs_source_replica2 = async {
        let token_generator = Box::new(get_service_account().await);
        let bucket = gcs_dst_t.bucket();
        let prefix = gcs_dst_t.prefix_as_folder();
        ReaderWriter::gcs(token_generator, bucket.as_str(), prefix.as_str())
            .await
            .unwrap()
    }
    .await;

    let gcs_dest_replica = generate_gcs(GcsTestConfig::from_env().await).await;

    let gcs_dest = generate_gcs(gcs_dst_t).await;

    let fs_source = Source::fs(src_t.base_path().as_path());
    let fs_source_replica = Source::fs(src_t.base_path().as_path());

    let fs_replica_t = FsTestConfig::new();
    let fs_dest_replica = Source::fs(fs_replica_t.base_path().as_path());

    let fs_replica_t2 = FsTestConfig::new();
    let fs_dest_replica2 = Source::fs(fs_replica_t2.base_path().as_path());

    let rsync_fs_to_gcs = RSync::new(fs_source, gcs_dest).with_restore_fs_mtime(set_fs_mtime);
    let rsync_gcs_to_gcs_replica =
        RSync::new(gcs_source_replica, gcs_dest_replica).with_restore_fs_mtime(set_fs_mtime);
    let rsync_fs_to_fs_replica =
        RSync::new(fs_source_replica, fs_dest_replica).with_restore_fs_mtime(set_fs_mtime);
    let rsync_gs_to_fs_replica =
        RSync::new(gcs_source_replica2, fs_dest_replica2).with_restore_fs_mtime(set_fs_mtime);

    let expected = vec![
        created("a/long/path/hello_world.toml"),
        created("hello/world/test.txt"),
        created("test.json"),
    ];
    assert_eq!(expected, sync(&rsync_fs_to_gcs).await);
    assert_eq!(expected, sync(&rsync_gcs_to_gcs_replica).await);
    assert_eq!(expected, sync(&rsync_fs_to_fs_replica).await);
    assert_eq!(expected, sync(&rsync_gs_to_fs_replica).await);

    let expected = vec![
        already_synced("same mtime and size", "a/long/path/hello_world.toml"),
        already_synced("same mtime and size", "hello/world/test.txt"),
        already_synced("same mtime and size", "test.json"),
    ];
    assert_eq!(expected, sync(&rsync_fs_to_gcs).await);
    assert_eq!(expected, sync(&rsync_gcs_to_gcs_replica).await);
    if set_fs_mtime {
        assert_eq!(expected, sync(&rsync_fs_to_fs_replica).await);
        assert_eq!(expected, sync(&rsync_gs_to_fs_replica).await);
    } else {
        let expected = vec![
            updated("different size or mtime", "a/long/path/hello_world.toml"),
            updated("different size or mtime", "hello/world/test.txt"),
            updated("different size or mtime", "test.json"),
        ];
        assert_eq!(expected, sync(&rsync_fs_to_fs_replica).await);
        assert_eq!(expected, sync(&rsync_gs_to_fs_replica).await);
    }

    write_to_file(src_t.file_path("test.json").as_path(), "updated").await;
    let new_file = src_t.file_path("new.json");
    write_to_file(new_file.as_path(), "new file").await;
    let expected = vec![
        created("new.json"),
        updated("different size or mtime", "test.json"),
        already_synced("same mtime and size", "a/long/path/hello_world.toml"),
        already_synced("same mtime and size", "hello/world/test.txt"),
    ];
    assert_eq!(expected, sync(&rsync_fs_to_gcs).await);
    assert_eq!(expected, sync(&rsync_gcs_to_gcs_replica).await);
    if set_fs_mtime {
        assert_eq!(expected, sync(&rsync_fs_to_fs_replica).await);
        assert_eq!(expected, sync(&rsync_gs_to_fs_replica).await);
    } else {
        let expected = vec![
            created("new.json"),
            updated("different size or mtime", "a/long/path/hello_world.toml"),
            updated("different size or mtime", "hello/world/test.txt"),
            updated("different size or mtime", "test.json"),
        ];
    
        assert_eq!(expected, sync(&rsync_fs_to_fs_replica).await);
        assert_eq!(expected, sync(&rsync_gs_to_fs_replica).await);
    }

    delete_files(&file_names[..]).await;
    let expected = vec![
        synced(already_synced("same mtime and size", "new.json")),
        deleted("a/long/path/hello_world.toml"),
        deleted("hello/world/test.txt"),
        deleted("test.json"),
        not_deleted("new.json"),
    ];
    assert_eq!(expected, mirror(&rsync_fs_to_gcs).await);
    assert_eq!(expected, mirror(&rsync_gcs_to_gcs_replica).await);
    if set_fs_mtime {
        assert_eq!(expected, mirror(&rsync_fs_to_fs_replica).await);
        assert_eq!(expected, mirror(&rsync_gs_to_fs_replica).await);
    } else {
        let expected = vec![
            synced(updated("different size or mtime", "new.json")),
            deleted("a/long/path/hello_world.toml"),
            deleted("hello/world/test.txt"),
            deleted("test.json"),
            not_deleted("new.json"),
        ];
        assert_eq!(expected, mirror(&rsync_fs_to_fs_replica).await);
        assert_eq!(expected, mirror(&rsync_gs_to_fs_replica).await);
    }

    delete_file(new_file.as_path()).await;
    let expected = vec![deleted("new.json")];
    assert_eq!(expected, mirror(&rsync_fs_to_gcs).await);
    assert_eq!(expected, mirror(&rsync_gcs_to_gcs_replica).await);
    assert_eq!(expected, mirror(&rsync_fs_to_fs_replica).await);
    assert_eq!(expected, mirror(&rsync_gs_to_fs_replica).await);
}

async fn test_include_and_exclude_rsync_conf_base(
    expected: Vec<RSyncStatus>,
    includes: &[&str],
    excludes: &[&str],
) {
    async fn generate_gcs(test_config: GcsTestConfig) -> Source {
        let bucket = test_config.bucket();
        let prefix = test_config.prefix_as_folder();

        Source::gcs(
            Box::new(test_config.token()),
            bucket.as_str(),
            prefix.as_str(),
        )
        .await
        .unwrap()
    }

    let fs = FsTestConfig::new();

    let file_names = vec![
        "/hello/world/test.txt",
        "test.json",
        "a/long/path/hello_world.toml",
    ]
    .into_iter()
    .map(|x| fs.file_path(x))
    .collect::<Vec<_>>();

    setup_files(&file_names[..], "Hello World").await;

    let gcs = generate_gcs(GcsTestConfig::from_env().await).await;
    let fs = Source::fs(fs.base_path().as_path());
    let rsync = RSync::new(fs, gcs)
        .with_includes(includes)
        .unwrap()
        .with_excludes(excludes)
        .unwrap();
    let actual = sync(&rsync).await;

    assert_eq!(expected, actual);
}

#[tokio::test]
async fn test_include_rsync_conf() {
    test_include_and_exclude_rsync_conf_base(
        vec![created("hello/world/test.txt")],
        vec![r#"hello/world/test.txt"#].as_slice(),
        vec![].as_slice(),
    )
    .await;
    test_include_and_exclude_rsync_conf_base(
        vec![created("hello/world/test.txt")],
        vec![r#"hello/**/test.txt"#].as_slice(),
        vec![].as_slice(),
    )
    .await;

    test_include_and_exclude_rsync_conf_base(
        vec![created("hello/world/test.txt")],
        vec!["*.txt"].as_slice(),
        vec![].as_slice(),
    )
    .await;
}

#[tokio::test]
async fn test_multiple_include_rsync_conf() {
    test_include_and_exclude_rsync_conf_base(
        vec![created("hello/world/test.txt"), created("test.json")],
        vec![r#"hello/**/test.txt"#, "test.json"].as_slice(),
        vec![].as_slice(),
    )
    .await;
}

#[tokio::test]
async fn test_exclude_all_rsync_conf() {
    test_include_and_exclude_rsync_conf_base(vec![], vec![].as_slice(), vec!["**"].as_slice())
        .await;
}

#[tokio::test]
async fn test_exclude_rsync_conf() {
    test_include_and_exclude_rsync_conf_base(
        vec![created("a/long/path/hello_world.toml")],
        vec![].as_slice(),
        vec!["*test.*"].as_slice(),
    )
    .await;
}

#[tokio::test]
async fn test_exclude_multiple_rsync_conf() {
    test_include_and_exclude_rsync_conf_base(
        vec![created("test.json")],
        vec![].as_slice(),
        vec!["a/**/hello_world.toml", "hello/**/test.*"].as_slice(),
    )
    .await;
}

#[tokio::test]
async fn test_mirror_include_and_exclude_rsync_conf() {
    async fn rw_gcs(bucket: &str, prefix: &str) -> Result<ReaderWriter, RSyncError> {
        let token_generator = Box::new(get_service_account().await);
        ReaderWriter::gcs(token_generator, bucket, prefix).await
    }

    let fs = FsTestConfig::new();

    let file_names = vec![
        "/hello/world/test.txt",
        "test.json",
        "a/long/path/hello_world.toml",
    ]
    .into_iter()
    .map(|x| fs.file_path(x))
    .collect::<Vec<_>>();

    setup_files(&file_names[..], "Hello World").await;

    let gcs_config = GcsTestConfig::from_env().await;
    let gcs_rw: ReaderWriter = rw_gcs(
        gcs_config.bucket().as_str(),
        gcs_config.prefix_as_folder().as_str(),
    )
    .await
    .unwrap();

    let fs_rw = Source::fs(fs.base_path().as_path());
    let rsync = RSync::new(fs_rw, gcs_rw);
    let actual = sync(&rsync).await;

    assert_eq!(
        vec![
            created("a/long/path/hello_world.toml"),
            created("hello/world/test.txt"),
            created("test.json"),
        ],
        actual
    );

    let gcs_rw: ReaderWriter = rw_gcs(
        gcs_config.bucket().as_str(),
        gcs_config.prefix_as_folder().as_str(),
    )
    .await
    .unwrap();

    let fs_rw = Source::fs(fs.base_path().as_path());
    let rsync = RSync::new(fs_rw, gcs_rw)
        .with_excludes(vec!["*.json"].as_slice())
        .unwrap();
    let actual = mirror(&rsync).await;
    assert_eq!(
        vec![
            synced(already_synced(
                "same mtime and size",
                "a/long/path/hello_world.toml"
            )),
            synced(already_synced(
                "same mtime and size",
                "hello/world/test.txt"
            )),
            deleted("test.json"),
            not_deleted("a/long/path/hello_world.toml"),
            not_deleted("hello/world/test.txt"),
        ],
        actual
    );
}
