use gcs_rsync::storage::{credentials, Object, ObjectClient, StorageResult};

#[tokio::main]
async fn main() -> StorageResult<()> {
    let args = std::env::args().collect::<Vec<_>>();
    let bucket = args[1].as_str();
    let name = args[2].as_str();
    let object = Object::new(bucket, name)?;

    let auc = Box::new(credentials::authorizeduser::default().await?);
    let object_client = ObjectClient::new(auc).await?;

    object_client.delete(&object).await?;
    println!("object {} uploaded", &object);
    Ok(())
}
