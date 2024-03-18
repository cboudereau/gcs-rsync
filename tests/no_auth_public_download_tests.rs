mod config;

use config::fs::FsTestConfig;
use futures::TryStreamExt;
use gcs_rsync::sync::{RSyncStatus, RelativePath};

#[tokio::test]
async fn test_public_download_without_any_auth() {
    let fs_test_config = FsTestConfig::new();
    let source = gcs_rsync::sync::ReaderWriter::gcs_no_auth("gcs-rsync-dev-public", "hello");
    let dest = gcs_rsync::sync::ReaderWriter::fs(&fs_test_config.base_path());
    let rsync = gcs_rsync::sync::RSync::new(source, dest);
    let sync_results = rsync
        .sync()
        .await
        .try_buffer_unordered(config::default::CONCURRENCY_LEVEL)
        .try_collect::<Vec<_>>()
        .await
        .unwrap();

    assert_eq!(
        vec![RSyncStatus::Created(
            RelativePath::new("hello.txt").unwrap()
        )],
        sync_results
    );

    let content = fs_test_config.read_to_string("hello.txt").await;

    assert_eq!("hello world", content);
}
