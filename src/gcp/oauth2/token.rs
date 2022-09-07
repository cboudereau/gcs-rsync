use crate::gcp::DeserializedResponse;
use crate::Client;

use super::{Error, TokenResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use std::{
    fmt::{Debug, Display},
    path::Path,
};

#[derive(Deserialize, Debug, Clone)]
pub struct Token {
    access_token: String,
    #[allow(dead_code)]
    token_type: String,
    #[serde(
        deserialize_with = "from_expires_in",
        rename(deserialize = "expires_in")
    )]
    expiry: DateTime<Utc>,
    #[serde(default)]
    scope: Option<String>,
}

fn from_expires_in<'de, D>(deserializer: D) -> std::result::Result<DateTime<Utc>, D::Error>
where
    D: Deserializer<'de>,
{
    let expires_in: i64 = Deserialize::deserialize(deserializer)?;
    Ok(Utc::now() + chrono::Duration::seconds(expires_in))
}

impl Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_valid() {
            write!(f, "Valid Token expires at {}", self.expiry)
        } else {
            write!(f, "Invalid Token expired at {}", self.expiry)
        }
    }
}

pub type AccessToken = String;

impl Token {
    pub fn access_token(&self) -> AccessToken {
        self.access_token.to_owned()
    }

    pub fn is_valid(&self) -> bool {
        self.expiry - chrono::Duration::seconds(30) > Utc::now()
    }

    pub fn with_scope(mut self, scope: String) -> Self {
        self.scope = Some(scope);
        self
    }
}

#[async_trait::async_trait]
pub trait TokenGenerator {
    async fn get(&self, client: &Client) -> TokenResult<Token>;
}

impl Debug for dyn TokenGenerator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

#[async_trait::async_trait]
impl TokenGenerator for AuthorizedUserCredentials {
    async fn get(&self, client: &Client) -> TokenResult<Token> {
        let req = self;
        let token: DeserializedResponse<Token> = client
            .client
            .post("https://accounts.google.com/o/oauth2/token")
            .json(&req)
            .send()
            .await
            .map_err(Error::HttpError)?
            .json()
            .await
            .map_err(Error::HttpError)?;
        token
            .into_result()
            .map_err(super::Error::unexpected_api_response::<Token>)
    }
}

#[async_trait::async_trait]
impl TokenGenerator for ServiceAccountCredentials {
    async fn get(&self, client: &Client) -> TokenResult<Token> {
        let now = chrono::Utc::now().timestamp();
        let exp = now + 3600;

        let scope = self.scope.to_owned().ok_or(super::Error::MissingScope)?;

        let claims = Claims {
            iss: self.client_email.as_str(),
            scope: scope.as_str(),
            aud: "https://www.googleapis.com/oauth2/v4/token",
            exp,
            iat: now,
        };
        let header = jsonwebtoken::Header {
            alg: jsonwebtoken::Algorithm::RS256,
            ..Default::default()
        };
        let private_key = jsonwebtoken::EncodingKey::from_rsa_pem(self.private_key.as_bytes())
            .map_err(Error::JWTError)?;
        let jwt = jsonwebtoken::encode(&header, &claims, &private_key).map_err(Error::JWTError)?;
        let form = [
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", &jwt),
        ];

        let token: DeserializedResponse<Token> = client
            .client
            .post("https://www.googleapis.com/oauth2/v4/token")
            .form(&form)
            .send()
            .await
            .map_err(Error::HttpError)?
            .json()
            .await
            .map_err(Error::HttpError)?;
        token
            .into_result()
            .map(|t| t.with_scope(scope))
            .map_err(super::Error::unexpected_api_response::<Token>)
    }
}

#[async_trait::async_trait]
impl TokenGenerator for GoogleMetadataServerCredentials {
    async fn get(&self, client: &Client) -> TokenResult<Token> {
        const DEFAULT_TOKEN_GCP_URI: &'static str = "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token";

        let token: DeserializedResponse<Token> = client
            .client
            .get(DEFAULT_TOKEN_GCP_URI)
            .header("Metadata-Flavor","Google")
            .send()
            .await
            .map_err(Error::HttpError)?
            .json()
            .await
            .map_err(Error::HttpError)?;
        token
            .into_result()
            .map_err(super::Error::unexpected_api_response::<Token>)
    }
}

fn from_str<T>(str: &str) -> TokenResult<T>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_str(str).map_err(Error::deserialization_error::<T>)
}

async fn from_file<T, U>(file_path: T) -> TokenResult<U>
where
    T: AsRef<Path>,
    U: serde::de::DeserializeOwned,
{
    tokio::fs::read_to_string(file_path.as_ref())
        .await
        .map_err(|err| Error::io_error("error while reading file", file_path.as_ref(), err))
        .and_then(|f| from_str(f.as_str()))
}

async fn default<T>() -> TokenResult<T>
where
    T: serde::de::DeserializeOwned,
{
    let default_path = {
        let key = "GOOGLE_APPLICATION_CREDENTIALS";
        std::env::var(key).map_err(|err| Error::env_var_error(key, err))?
    };
    from_file(default_path).await
}

#[derive(Serialize, Debug)]
struct Claims<'a> {
    iss: &'a str,
    aud: &'a str,
    exp: i64,
    iat: i64,
    scope: &'a str,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct AuthorizedUserCredentials {
    client_id: String,
    client_secret: String,
    refresh_token: String,
    #[serde(default = "refresh_token")]
    grant_type: String,
}

fn refresh_token() -> String {
    "refresh_token".to_owned()
}

impl AuthorizedUserCredentials {
    pub fn from(s: &str) -> TokenResult<Self> {
        from_str(s)
    }

    pub async fn from_file<T>(file_path: T) -> TokenResult<Self>
    where
        T: AsRef<Path>,
    {
        from_file(file_path).await
    }

    pub async fn default() -> TokenResult<Self> {
        default().await
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct ServiceAccountCredentials {
    r#type: String,
    project_id: String,
    private_key_id: String,
    private_key: String,
    client_email: String,
    client_id: String,
    auth_uri: String,
    token_uri: String,
    auth_provider_x509_cert_url: String,
    client_x509_cert_url: String,
    #[serde(default)]
    scope: Option<String>,
}

impl ServiceAccountCredentials {
    pub fn from(s: &str) -> TokenResult<Self> {
        from_str(s)
    }

    pub async fn from_file<T>(file_path: T) -> TokenResult<Self>
    where
        T: AsRef<Path>,
    {
        from_file(file_path).await
    }

    pub async fn default() -> TokenResult<Self> {
        default().await
    }

    pub fn with_scope(mut self, scope: &str) -> Self {
        self.scope = Some(scope.to_owned());
        self
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct GoogleMetadataServerCredentials {
}

impl GoogleMetadataServerCredentials {

    pub fn default() -> TokenResult<Self> {
        Ok(GoogleMetadataServerCredentials{})
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Not;

    use crate::gcp::oauth2::token::*;

    #[test]
    fn token_from_json_test() {
        let raw = r#"{
            "access_token": "access_token",
            "expires_in": 3599,
            "scope": "scope",
            "token_type": "Bearer",
            "id_token": "id_token"
        }"#;

        let actual: Token = serde_json::from_str(raw).unwrap();
        assert_eq!("access_token", actual.access_token);
        assert_eq!("Bearer", actual.token_type);
        assert!(actual.expiry > Utc::now());
    }
    #[test]
    fn token_from_authorized_user_json_test() {
        let actual = super::from_str(
            r#"{
                   "client_id": "client_id",
                   "client_secret": "client_secret",
                   "quota_project_id": "quota_project_id",
                   "refresh_token": "refresh_token",
                   "type": "authorized_user"
            }"#,
        )
        .unwrap();
        let au = AuthorizedUserCredentials {
            client_id: "client_id".to_owned(),
            client_secret: "client_secret".to_owned(),
            refresh_token: "refresh_token".to_owned(),
            grant_type: "refresh_token".to_owned(),
        };

        assert_eq!(au, actual);
    }

    #[test]
    fn token_from_service_account_json_test() {
        let actual = super::from_str(
            r#"{
                "type": "service_account",
                "project_id": "project_id",
                "private_key_id": "private_key_id",
                "private_key": "private_key",
                "client_email": "client_email",
                "client_id": "client_id",
                "auth_uri": "auth_uri",
                "token_uri": "token_uri",
                "auth_provider_x509_cert_url": "auth_provider_x509_cert_url",
                "client_x509_cert_url": "client_x509_cert_url"
            }"#,
        )
        .unwrap();
        let sa = ServiceAccountCredentials {
            r#type: "service_account".to_owned(),
            project_id: "project_id".to_owned(),
            private_key_id: "private_key_id".to_owned(),
            private_key: "private_key".to_owned(),
            client_email: "client_email".to_owned(),
            client_id: "client_id".to_owned(),
            auth_uri: "auth_uri".to_owned(),
            token_uri: "token_uri".to_owned(),
            auth_provider_x509_cert_url: "auth_provider_x509_cert_url".to_owned(),
            client_x509_cert_url: "client_x509_cert_url".to_owned(),
            scope: None,
        };

        assert_eq!(sa, actual);
    }

    #[test]
    fn test_token_is_valid_false() {
        let token = Token {
            access_token: "Hello".to_owned(),
            token_type: "token type".to_owned(),
            expiry: chrono::Utc::now(),
            scope: None,
        };

        assert!(token.is_valid().not());
        assert!(
            format!("{}", token).starts_with("Invalid Token expired at"),
            "expected an invalid token but got {}",
            token
        )
    }

    #[test]
    fn test_token_is_valid_true() {
        let token = Token {
            access_token: "Hello".to_owned(),
            token_type: "token type".to_owned(),
            expiry: chrono::Utc::now() + chrono::Duration::seconds(35),
            scope: None,
        };

        assert!(token.is_valid());
        assert!(
            format!("{}", token).starts_with("Valid Token expires at"),
            "expected a valid token but got {}",
            token
        )
    }
}
