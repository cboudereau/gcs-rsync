mod config;

use config::fs::FsTestConfig;
use futures::{StreamExt, TryStreamExt};
use gcs_rsync::sync::{RMirrorStatus, RSyncError, RSyncStatus, RelativePath};

#[tokio::test]
async fn test_public_bucket_mirror_when_fs_source_does_not_exist() {
    let fs_test_config = FsTestConfig::new();
    let source = gcs_rsync::sync::ReaderWriter::gcs_no_auth("gcs-rsync-dev-public", "hello");
    let dest = gcs_rsync::sync::ReaderWriter::fs(&fs_test_config.base_path());

    let rsync = gcs_rsync::sync::RSync::new(source, dest);
    let r = rsync.mirror().await;

    let r2 = r.unwrap();

    let mut oks = Vec::new();

    let mut fs_io_errors = Vec::new();
    r2.try_buffer_unordered(config::default::CONCURRENCY_LEVEL)
        .for_each(|r| {
            match r {
                Ok(x) => oks.push(x),
                Err(x) => {
                    if let RSyncError::FsIoError { .. } = x {
                        fs_io_errors.push(x)
                    };
                }
            }
            futures::future::ready(())
        })
        .await;

    assert_eq!(
        vec![RMirrorStatus::Synced(RSyncStatus::Created(
            RelativePath::new("hello.txt").unwrap()
        ))],
        oks
    );

    assert_eq!(1, fs_io_errors.len());

    let content = fs_test_config.read_to_string("hello.txt").await;

    assert_eq!("hello world", content);
}
