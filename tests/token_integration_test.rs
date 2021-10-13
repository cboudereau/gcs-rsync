use gcs_rsync::{
    oauth2::token::{AuthorizedUserCredentials, ServiceAccountCredentials, TokenGenerator},
    Client,
};

#[tokio::test]
async fn test_token_dev() {
    let client = Client::default();
    let auc = AuthorizedUserCredentials::default().await.unwrap();
    let t = auc.get(&client).await.unwrap();

    assert!(t.is_valid());
}

#[tokio::test]
async fn test_token_service_account() {
    let client = Client::default();
    let path = env!("TEST_SERVICE_ACCOUNT");
    let sac = ServiceAccountCredentials::from_file(path).await.unwrap();
    let t = sac
        .with_scope("https://www.googleapis.com/auth/devstorage.full_control")
        .get(&client)
        .await
        .unwrap();
    assert!(t.is_valid());
}

#[tokio::test]
async fn test_token_service_account_with_detailed_error() {
    let client = Client::default();
    let path = env!("TEST_SERVICE_ACCOUNT");
    let sac = ServiceAccountCredentials::from_file(path).await.unwrap();
    match sac.with_scope("").get(&client).await.unwrap_err() {
        gcs_rsync::oauth2::Error::UnexpectedApiResponse(actual) => {
            let expected: serde_json::Value =
            serde_json::from_str(r#"{ "error": "invalid_scope", "error_description": "Invalid OAuth scope or ID token audience provided." }"#)
                .unwrap();
            assert_eq!(expected, actual);
        }
        e => {
            panic!("expected an UnexpectedApiResponse but got {:?}", e);
        }
    }
}
