use futures::{StreamExt, TryStreamExt};
use gcs_rsync::{
    storage::credentials::authorizeduser,
    sync::{RSync, RSyncResult, ReaderWriter},
};

#[tokio::main]
async fn main() -> RSyncResult<()> {
    let token_generator = Box::new(authorizeduser::default().await.unwrap());

    let test_prefix = env!("EXAMPLE_PREFIX");
    let bucket = env!("EXAMPLE_BUCKET");

    let source = ReaderWriter::gcs(token_generator, bucket, test_prefix)
        .await
        .unwrap();

    let token_generator = Box::new(authorizeduser::default().await.unwrap());
    let dest_prefix = format!("{}_dest", test_prefix);
    let dest = ReaderWriter::gcs(token_generator, bucket, &dest_prefix)
        .await
        .unwrap();

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
