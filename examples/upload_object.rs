use std::path::Path;

use gcs_sync::storage::{credentials, Object, ObjectClient, StorageResult};
use tokio_util::codec::{BytesCodec, FramedRead};

#[tokio::main]
async fn main() -> StorageResult<()> {
    let args = std::env::args().collect::<Vec<_>>();
    let bucket = args[1].as_str();
    let prefix = args[2].to_owned();
    let file_path = args[3].to_owned();

    let auc = credentials::authorizeduser::default().await?;
    let object_client = ObjectClient::new(auc).await?;

    let file_path = Path::new(&file_path);
    let name = file_path.file_name().unwrap().to_string_lossy();

    let file = tokio::fs::File::open(file_path).await.unwrap();
    let stream = FramedRead::new(file, BytesCodec::new());

    let name = format!("{}/{}", prefix, name);
    let object = Object::new(bucket, name.as_str());
    object_client.upload(&object, stream).await.unwrap();
    println!("object {} uploaded", &object);
    Ok(())
}
