use futures::TryStreamExt;
use gcs_rsync::storage::{credentials, ObjectClient, ObjectsListRequest, StorageResult};

#[tokio::main]
async fn main() -> StorageResult<()> {
    let args = std::env::args().collect::<Vec<_>>();
    let bucket = args[1].as_str();
    let prefix = args[2].to_owned();

    let auc = Box::new(credentials::authorizeduser::default().await?);
    let object_client = ObjectClient::new(auc).await?;

    let objects_list_request = ObjectsListRequest {
        prefix: Some(prefix),
        fields: Some("items(name),nextPageToken".to_owned()),
        ..Default::default()
    };

    object_client
        .list(bucket, &objects_list_request)
        .await
        .try_for_each(|x| {
            println!("{}", x.name.unwrap());
            futures::future::ok(())
        })
        .await?;

    Ok(())
}
