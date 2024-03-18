use std::path::Path;

use futures::TryStreamExt;
use gcs_rsync::storage::{Object, ObjectClient, StorageResult};
use tokio::{
    fs::File,
    io::{AsyncWriteExt, BufWriter},
};

#[tokio::main]
async fn main() -> StorageResult<()> {
    let bucket = "gcs-rsync-dev-public";
    let name = "hello.txt";

    let object_client = ObjectClient::no_auth();

    let file_name = Path::new(&name).file_name().unwrap().to_string_lossy();
    let file_path = file_name.to_string();

    let object = Object::new(
        bucket,
        "hello.txt",
    )?;
    let mut stream = object_client.download(&object).await.unwrap();

    let file = File::create(&file_path).await.unwrap();
    let mut buf_writer = BufWriter::new(file);

    while let Some(data) = stream.try_next().await.unwrap() {
        buf_writer.write_all(&data).await.unwrap();
    }

    buf_writer.flush().await.unwrap();
    println!("object {} downloaded to {:?}", &object, file_path);
    Ok(())
}
