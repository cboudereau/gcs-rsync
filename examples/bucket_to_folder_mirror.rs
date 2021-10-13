use std::{path::PathBuf, str::FromStr};

use futures::{StreamExt, TryStreamExt};
use gcs_rsync::{
    storage::credentials::authorizeduser,
    sync::{RMirrorStatus, RSync, RSyncResult, ReaderWriter},
};

#[tokio::main]
async fn main() -> RSyncResult<()> {
    let token_generator = authorizeduser::default().await.unwrap();

    let home_dir = env!("HOME");
    let test_prefix = env!("EXAMPLE_PREFIX");
    let bucket = env!("EXAMPLE_BUCKET");

    let source = ReaderWriter::gcs(token_generator, bucket, test_prefix)
        .await
        .unwrap();

    let dest_folder = {
        let mut p = PathBuf::from_str(home_dir).unwrap();
        p.push(test_prefix);
        p
    };
    let dest = ReaderWriter::fs(dest_folder.as_path());

    let rsync = RSync::new(source, dest);

    rsync
        .mirror()
        .await
        .try_buffer_unordered(12)
        .try_filter(|x| match *x {
            RMirrorStatus::NotDeleted(_) => futures::future::ready(false),
            _ => futures::future::ready(true),
        })
        .for_each(|x| {
            println!("{:?}", x);
            futures::future::ready(())
        })
        .await;

    Ok(())
}
