use std::ops::Not;

use futures::{StreamExt, TryStreamExt};
use gcs_rsync::{
    oauth2::token::AuthorizedUserCredentials,
    storage::{
        credentials, Object, ObjectClient, ObjectsListRequest, PartialObject, StorageResult,
    },
};

mod config;
use config::gcs::GcsTestConfig;

#[tokio::test]
async fn test_test_config() {
    let t = GcsTestConfig::from_env().await;

    assert!(
        t.list_prefix().is_empty().not(),
        "list prefix should not be empty"
    );

    assert!(
        t.object("object_name").name.ends_with("/object_name"),
        "object name should end with /object_name"
    );

    assert!(t.bucket().is_empty().not(), "bucket should not be empty");
}

async fn assert_delete_err(
    object_client: &ObjectClient<AuthorizedUserCredentials>,
    object: &Object,
) {
    let delete_result = object_client.delete(object).await;
    assert!(
        delete_result.is_err(),
        "expected an error got {:?} for {}",
        delete_result,
        object
    );
}

async fn assert_delete_ok(
    object_client: &ObjectClient<AuthorizedUserCredentials>,
    object: &Object,
) {
    let delete_result = object_client.delete(object).await;
    assert!(
        delete_result.is_ok(),
        "unexpected error {:?} for {}",
        delete_result,
        object
    );
}

async fn upload_bytes(
    object_client: &ObjectClient<AuthorizedUserCredentials>,
    object: &Object,
    content: &str,
) -> StorageResult<()> {
    let data = bytes::Bytes::copy_from_slice(content.as_bytes());
    let stream = futures::stream::once(futures::future::ok::<bytes::Bytes, String>(data));
    object_client.upload(object, stream).await
}

async fn assert_upload_bytes(
    object_client: &ObjectClient<AuthorizedUserCredentials>,
    object: &Object,
    content: &str,
) {
    let upload_result = upload_bytes(object_client, object, content).await;
    assert!(
        upload_result.is_ok(),
        "unexpected error got {:?} for {}",
        upload_result,
        object
    );
}

async fn assert_download_bytes(
    object_client: &ObjectClient<AuthorizedUserCredentials>,
    object: &Object,
    expected: &str,
) {
    let bytes = futures::stream::once(object_client.download(object))
        .try_flatten()
        .try_fold(Vec::new(), |mut bytes, buffer| {
            for byte in buffer {
                bytes.push(byte);
            }
            futures::future::ok(bytes)
        })
        .await
        .unwrap();
    let actual = String::from_utf8(bytes).unwrap();
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn test_delete_upload_download_delete() {
    let test_config = GcsTestConfig::from_env().await;
    let object = test_config.object("object.txt");
    let object_client = ObjectClient::new(test_config.token()).await.unwrap();

    let content = "hello";
    assert_delete_err(&object_client, &object).await;
    assert_upload_bytes(&object_client, &object, content).await;
    assert_download_bytes(&object_client, &object, content).await;
    assert_delete_ok(&object_client, &object).await;
}

#[tokio::test]
async fn test_get_object_ok() {
    let test_config = GcsTestConfig::from_env().await;
    let object = test_config.object("object.txt");
    let object_client = ObjectClient::new(test_config.token()).await.unwrap();

    let content = "hello";
    assert_delete_err(&object_client, &object).await;
    assert_upload_bytes(&object_client, &object, content).await;

    let partial_object = object_client.get(&object, "name,selfLink").await.unwrap();

    assert!(partial_object.name.unwrap().ends_with("object.txt"));
    assert!(partial_object.self_link.unwrap().ends_with("%2Fobject.txt"));
    assert_eq!(None, partial_object.crc32c);
    assert_delete_ok(&object_client, &object).await;
}

#[tokio::test]
async fn test_get_object_not_found() {
    let test_config = GcsTestConfig::from_env().await;
    let object = test_config.object("object.txt");
    let object_client = ObjectClient::new(test_config.token()).await.unwrap();

    let err = object_client
        .get(&object, "name,selfLink")
        .await
        .unwrap_err();

    assert_not_found_response(err);
}

#[tokio::test]
async fn test_upload_with_detailed_error() {
    let test_config = GcsTestConfig::from_env().await;
    let object_client = ObjectClient::new(test_config.token()).await.unwrap();
    let object = Object::new("the_bad_bucket", "name").unwrap();

    let err = upload_bytes(&object_client, &object, "").await.unwrap_err();

    assert_unexpected_response(err, "forbidden");
}

#[tokio::test]
async fn test_api_list_objects() {
    let test_config = GcsTestConfig::from_env().await;

    let count = 11;
    let prefix = test_config.list_prefix();
    let bucket = test_config.bucket();
    let test_objects = (0..count + 2)
        .map(|i| test_config.object(format!("object_{}", i).as_str()))
        .collect::<Vec<_>>();

    let object_client = ObjectClient::new(test_config.token()).await.unwrap();
    futures::stream::iter(test_objects.iter())
        .for_each_concurrent(config::default::CONCURRENCY_LEVEL, |object| {
            assert_upload_bytes(&object_client, object, "hello")
        })
        .await;

    let objects_list_request = ObjectsListRequest {
        prefix: Some(prefix),
        fields: Some("items(selfLink,name),nextPageToken".to_owned()),
        max_results: Some(2),
        ..Default::default()
    };

    let result: Vec<PartialObject> = object_client
        .list(bucket.as_str(), &objects_list_request)
        .await
        .take(count)
        .try_collect()
        .await
        .unwrap();

    assert_eq!(count, result.len());

    futures::stream::iter(test_objects.iter())
        .for_each_concurrent(config::default::CONCURRENCY_LEVEL, |object| {
            assert_delete_ok(&object_client, object)
        })
        .await;
}

#[tokio::test]
async fn test_crc32c_object() {
    let test_config = GcsTestConfig::from_env().await;
    let bucket = test_config.bucket();
    let prefix = test_config.list_prefix();
    let test_object = &test_config.object("test_crc32c");
    let object_client = ObjectClient::new(test_config.token()).await.unwrap();
    assert_upload_bytes(&object_client, test_object, "hello world!").await;

    let objects_list_request = ObjectsListRequest {
        prefix: Some(prefix),
        fields: Some("items(name,crc32c),nextPageToken".to_owned()),
        max_results: Some(2),
        ..Default::default()
    };

    let mut result: Vec<PartialObject> = object_client
        .list(bucket.as_str(), &objects_list_request)
        .await
        .try_collect()
        .await
        .unwrap();

    assert_eq!(1, result.len());
    let crc32c = result.pop().unwrap().crc32c.unwrap_or_default().to_u32();
    assert_eq!(1238062967, crc32c);
    assert_delete_ok(&object_client, test_object).await;
}

fn assert_unexpected_response(err: gcs_rsync::storage::Error, content: &str) {
    match err {
        gcs_rsync::storage::Error::GcsUnexpectedResponse(actual) => {
            assert!(
                actual.to_string().contains(content),
                "{:?} not found in json {}",
                content,
                actual
            );
        }
        e => panic!("expected UnexpectedApiResponse error got {:?}", e),
    }
}

fn assert_not_found_response(err: gcs_rsync::storage::Error) {
    match err {
        gcs_rsync::storage::Error::GcsResourceNotFound { .. } => (),
        e => panic!("expected GcsResourceNotFound error got {:?}", e),
    }
}

#[tokio::test]
async fn test_api_list_objects_not_found_error() {
    let auc = credentials::authorizeduser::default().await.unwrap();

    let object_client = ObjectClient::new(auc).await.unwrap();

    let objects_list_request = ObjectsListRequest::default();

    let err = object_client
        .list("my_very_bad_bucket", &objects_list_request)
        .await
        .take(1)
        .try_collect::<Vec<_>>()
        .await
        .unwrap_err();

    assert_not_found_response(err);
}
