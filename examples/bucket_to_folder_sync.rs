use std::{path::PathBuf, str::FromStr};

use futures::{StreamExt, TryStreamExt};
use gcs_sync::{
    storage::credentials::authorizeduser,
    sync::{RSync, RSyncResult, ReaderWriter},
};

#[tokio::main]
async fn main() -> RSyncResult<()> {
    let token_generator = authorizeduser::default().await.unwrap();

    let source = ReaderWriter::gcs(token_generator, "test", "sync_test3/")
        .await
        .unwrap();

    let dest_folder = PathBuf::from_str("/test3").unwrap();
    let dest = ReaderWriter::fs(dest_folder.as_path());

    let rsync = RSync::new(source, dest);

    rsync
        .sync()
        .await
        .try_buffer_unordered(12)
        .for_each(|x| {
            println!("{:?}", x);
            futures::future::ready(())
        })
        .await;

    Ok(())
}
