use std::{convert::TryInto, fmt::Display, str::FromStr};

use base64::Engine;

use crate::storage::{Error, StorageResult};

#[derive(Debug, PartialEq, Eq, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub enum Projection {
    Full,
    NoAcl,
}

/// See [GCS list API reference](https://cloud.google.com/storage/docs/json_api/v1/objects/list)
#[derive(Debug, PartialEq, Eq, serde::Serialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ObjectsListRequest {
    /// [Partial Response](https://cloud.google.com/storage/docs/json_api#partial-response)
    pub fields: Option<String>,
    pub delimiter: Option<String>,
    pub end_offset: Option<String>,
    pub include_trailing_delimiter: Option<bool>,
    pub max_results: Option<usize>,
    pub page_token: Option<String>,
    pub prefix: Option<String>,
    pub projection: Option<Projection>,
    pub start_offset: Option<String>,
    pub versions: Option<bool>,
}

#[derive(Debug, PartialEq, Eq, serde::Serialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ObjectMetadata {
    pub metadata: Metadata,
}
#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    #[serde(
        rename = "goog-reserved-file-mtime",
        deserialize_with = "from_string_option"
    )] //compat with gsutil rsync
    pub modification_time: Option<i64>,
}

/// ObjectList response
#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Objects {
    pub kind: Option<String>,

    #[serde(default = "Vec::new")]
    pub items: Vec<PartialObject>,

    #[serde(default = "Vec::new")]
    pub prefixes: Vec<String>,

    pub next_page_token: Option<String>,
}

#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Object {
    pub bucket: String,
    pub name: String,
}

impl Display for Object {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.gs_url())
    }
}

impl FromStr for Object {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.strip_prefix("gs://")
            .and_then(|part| part.split_once('/'))
            .ok_or(Error::GcsInvalidUrl {
                url: s.to_owned(),
                message: "gs url should be gs://bucket/object/path/name".to_owned(),
            })
            .and_then(|(bucket, name)| Object::new(bucket, name))
    }
}

type GsUrl = String;

const BASE_URL: &str = "https://storage.googleapis.com/storage/v1";
const UPLOAD_BASE_URL: &str = "https://storage.googleapis.com/upload/storage/v1";

fn percent_encode(input: &str) -> String {
    percent_encoding::utf8_percent_encode(input, percent_encoding::NON_ALPHANUMERIC).to_string()
}

impl Object {
    pub fn gs_url(&self) -> GsUrl {
        format!("gs://{}/{}", &self.bucket, &self.name)
    }

    /// References: `<https://cloud.google.com/storage/docs/naming-objects>`
    pub fn new(bucket: &str, name: &str) -> StorageResult<Self> {
        if bucket.is_empty() {
            return Err(Error::GcsInvalidObjectName);
        }

        if name.is_empty() || name.starts_with('.') {
            return Err(Error::GcsInvalidObjectName);
        }

        Ok(Self {
            bucket: bucket.to_owned(),
            name: name.to_owned(),
        })
    }

    pub fn url(&self) -> String {
        format!(
            "{}/b/{}/o/{}",
            BASE_URL,
            percent_encode(&self.bucket),
            percent_encode(&self.name)
        )
    }

    pub fn upload_url(&self, upload_type: &str) -> String {
        format!(
            "{}/b/{}/o?uploadType={}&name={}",
            UPLOAD_BASE_URL,
            percent_encode(&self.bucket),
            upload_type,
            percent_encode(&self.name)
        )
    }
}

#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bucket {
    name: String,
}

impl Bucket {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
        }
    }

    pub fn url(&self) -> String {
        format!("{}/b/{}/o", BASE_URL, percent_encode(&self.name))
    }
}

impl TryInto<Object> for PartialObject {
    type Error = Error;

    fn try_into(self) -> Result<Object, Self::Error> {
        fn err(e: &str) -> Error {
            Error::GcsPartialResponseError(e.to_owned())
        }

        match (self.bucket, self.name) {
            (Some(bucket), Some(name)) => Ok(Object { bucket, name }),
            (None, Some(_)) => Err(err("bucket field is missing")),
            (Some(_), None) => Err(err("name field is missing")),
            (None, None) => Err(err("bucket and name fields are missing")),
        }
    }
}

#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PartialObject {
    pub bucket: Option<String>,
    pub id: Option<String>,
    pub self_link: Option<String>,
    pub name: Option<String>,
    pub content_type: Option<String>,
    pub time_created: Option<chrono::DateTime<chrono::Utc>>,
    pub updated: Option<chrono::DateTime<chrono::Utc>>,
    pub time_deleted: Option<chrono::DateTime<chrono::Utc>>,
    pub retention_expiration_time: Option<chrono::DateTime<chrono::Utc>>,
    pub storage_class: Option<String>,
    #[serde(default, deserialize_with = "from_string_option")]
    pub size: Option<u64>,
    pub media_link: Option<String>,
    pub content_encoding: Option<String>,
    pub content_disposition: Option<String>,
    pub content_language: Option<String>,
    pub cache_control: Option<String>,
    pub metadata: Option<Metadata>,
    #[serde(default, deserialize_with = "from_string_option")]
    pub crc32c: Option<CRC32C>,
    pub etag: Option<String>,
}

#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CRC32C {
    value: u32,
}

impl CRC32C {
    pub fn new(value: u32) -> Self {
        Self { value }
    }
    pub fn to_u32(&self) -> u32 {
        self.value
    }
}
#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Base64EncodedCRC32CError {
    Base64DecodeError(String),
    Base64ToU32BigEndianError(Vec<u8>),
}

impl Display for Base64EncodedCRC32CError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl FromStr for CRC32C {
    type Err = Base64EncodedCRC32CError;

    fn from_str(base64crc32c: &str) -> Result<Self, Self::Err> {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(base64crc32c)
            .map_err(|err| Base64EncodedCRC32CError::Base64DecodeError(format!("{:?}", err)))?;
        let crc32c = decoded
            .try_into()
            .map(u32::from_be_bytes)
            .map_err(Base64EncodedCRC32CError::Base64ToU32BigEndianError)?;
        Ok(CRC32C::new(crc32c))
    }
}

fn from_string_option<'de, T, D>(deserializer: D) -> std::result::Result<Option<T>, D::Error>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
    D: serde::Deserializer<'de>,
{
    use serde::{de::Error, Deserialize};
    use serde_json::Value;
    match Deserialize::deserialize(deserializer) {
        Ok(Value::String(s)) => T::from_str(&s).map(Option::from).map_err(Error::custom),
        Ok(Value::Number(num)) => T::from_str(&num.to_string())
            .map(Option::from)
            .map_err(Error::custom),
        Ok(value) => Err(Error::custom(format!(
            "Wrong type, expected type {} but got value {:?}",
            std::any::type_name::<T>(),
            value,
        ))),
        Err(_) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use std::{convert::TryInto, str::FromStr};

    use crate::storage::{Bucket, Error, Object};

    use super::PartialObject;

    #[test]
    fn fn_gs_url_parsing_to_object() {
        assert_eq!(
            Object::new("hello", "world/object").unwrap(),
            Object::from_str("gs://hello/world/object").unwrap(),
        )
    }

    #[test]
    fn test_invalid_object() {
        fn assert_object_error(bucket: &str, name: &str) {
            assert!(matches!(
                Object::new(bucket, name).unwrap_err(),
                Error::GcsInvalidObjectName
            ))
        }
        assert_object_error("", "name");
        assert_object_error("bucket", "");
        assert_object_error("bucket", ".");
        assert_object_error("bucket", "..");
    }
    #[test]
    fn test_object_display() {
        let o = Object::new("hello", "world").unwrap();
        assert_eq!("gs://hello/world", o.gs_url());
        assert_eq!("gs://hello/world", format!("{}", o));
    }

    #[test]
    fn test_object_url() {
        let o = Object::new("hello/hello", "world/world").unwrap();
        assert_eq!(
            "https://storage.googleapis.com/storage/v1/b/hello%2Fhello/o/world%2Fworld",
            o.url()
        );
    }

    #[test]
    fn test_object_upload_url() {
        let o = Object::new("hello/hello", "world/world").unwrap();
        assert_eq!(
            "https://storage.googleapis.com/upload/storage/v1/b/hello%2Fhello/o?uploadType=media&name=world%2Fworld",
            o.upload_url("media")
        );
    }

    #[test]
    fn test_bucket_url() {
        let b = Bucket::new("hello/hello");
        assert_eq!(
            "https://storage.googleapis.com/storage/v1/b/hello%2Fhello/o",
            b.url()
        );
    }

    #[test]
    fn test_partial_object_into_object() {
        let p = PartialObject {
            bucket: Some("hello".to_owned()),
            name: Some("world".to_owned()),
            ..Default::default()
        };

        assert_eq!(
            Object::new("hello", "world").unwrap(),
            p.try_into().unwrap()
        );
    }
}
