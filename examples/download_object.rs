use std::path::Path;

use futures::TryStreamExt;
use gcs_rsync::storage::{credentials, Object, ObjectClient, StorageResult};
use tokio::{
    fs::File,
    io::{AsyncWriteExt, BufWriter},
};

#[tokio::main]
async fn main() -> StorageResult<()> {
    let args = std::env::args().collect::<Vec<_>>();
    let bucket = args[1].as_str();
    let name = args[2].as_str();
    let output_path = args[3].to_owned();

    let auc = credentials::authorizeduser::default().await?;
    let object_client = ObjectClient::new(auc).await?;

    let file_name = Path::new(&name).file_name().unwrap().to_string_lossy();
    let file_path = format!("{}/{}", output_path, file_name);

    let object = Object::new(bucket, name)?;
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
