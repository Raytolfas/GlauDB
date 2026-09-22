use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use sha2::{Sha256, Digest};
use thiserror::Error;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Error)]
pub enum S3Error {
    #[error("Reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("Invalid URL: {0}")]
    UrlParse(#[from] url::ParseError),
    #[error("HMAC signature error: {0}")]
    SignatureError(String),
    #[error("Upload failed with status {0}: {1}")]
    UploadFailed(u16, String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Config {
    pub enabled: bool,
    pub bucket: String,
    pub region: String,
    pub endpoint: String,
    pub access_key: String,
    pub secret_key: String,
    pub force_path_style: bool,
}

impl Default for S3Config {
    fn default() -> Self {
        Self {
            enabled: false,
            bucket: String::new(),
            region: "us-east-1".to_string(),
            endpoint: "https://s3.amazonaws.com".to_string(),
            access_key: String::new(),
            secret_key: String::new(),
            force_path_style: true,
        }
    }
}

pub struct S3Client {
    config: S3Config,
}

impl S3Client {
    pub fn new(config: S3Config) -> Self {
        Self { config }
    }

    fn sha256_hash(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }

    fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
        let mut mac = HmacSha256::new_from_slice(key).unwrap();
        mac.update(data);
        mac.finalize().into_bytes().to_vec()
    }

    fn sign(
        &self,
        method: &str,
        uri: &str,
        headers: &[(String, String)],
        payload_hash: &str,
        now: DateTime<Utc>,
    ) -> Result<String, S3Error> {
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let datestamp = now.format("%Y%m%d").to_string();

        let mut sorted_headers = headers.to_vec();
        sorted_headers.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

        let mut canonical_headers = String::new();
        let mut signed_headers_list = Vec::new();
        for (k, v) in &sorted_headers {
            let k_low = k.to_lowercase();
            canonical_headers.push_str(&format!("{}:{}\n", k_low, v.trim()));
            signed_headers_list.push(k_low);
        }
        let signed_headers = signed_headers_list.join(";");

        let canonical_request = format!(
            "{}\n{}\n\n{}\n{}\n{}",
            method,
            uri,
            canonical_headers,
            signed_headers,
            payload_hash
        );

        let canonical_request_hash = Self::sha256_hash(canonical_request.as_bytes());

        let credential_scope = format!("{}/{}/s3/aws4_request", datestamp, self.config.region);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{}\n{}\n{}",
            amz_date,
            credential_scope,
            canonical_request_hash
        );

        let k_secret = format!("AWS4{}", self.config.secret_key);
        let k_date = Self::hmac_sha256(k_secret.as_bytes(), datestamp.as_bytes());
        let k_region = Self::hmac_sha256(&k_date, self.config.region.as_bytes());
        let k_service = Self::hmac_sha256(&k_region, b"s3");
        let k_signing = Self::hmac_sha256(&k_service, b"aws4_request");

        let signature = hex::encode(Self::hmac_sha256(&k_signing, string_to_sign.as_bytes()));

        let authorization_header = format!(
            "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
            self.config.access_key,
            credential_scope,
            signed_headers,
            signature
        );

        Ok(authorization_header)
    }

    pub async fn upload_file(
        &self,
        key: &str,
        data: &[u8],
        content_type: &str,
    ) -> Result<String, S3Error> {
        let now = Utc::now();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();

        let host = if self.config.force_path_style {
            let url = url::Url::parse(&self.config.endpoint)?;
            url.host_str().unwrap_or("s3.amazonaws.com").to_string()
        } else {
            format!("{}.{}", self.config.bucket, self.config.endpoint.trim_start_matches("https://").trim_start_matches("http://"))
        };

        let path = if self.config.force_path_style {
            format!("/{}/{}", self.config.bucket, key.trim_start_matches('/'))
        } else {
            format!("/{}", key.trim_start_matches('/'))
        };

        let request_url = format!(
            "{}{}",
            self.config.endpoint.trim_end_matches('/'),
            if self.config.force_path_style {
                format!("/{}/{}", self.config.bucket, key.trim_start_matches('/'))
            } else {
                format!("/{}", key.trim_start_matches('/'))
            }
        );

        let payload_hash = Self::sha256_hash(data);

        let mut headers = vec![
            ("host".to_string(), host.clone()),
            ("x-amz-date".to_string(), amz_date.clone()),
            ("x-amz-content-sha256".to_string(), payload_hash.clone()),
            ("content-type".to_string(), content_type.to_string()),
        ];

        let auth_header = self.sign("PUT", &path, &headers, &payload_hash, now)?;
        headers.push(("authorization".to_string(), auth_header));

        let client = reqwest::Client::new();
        let mut request = client.put(&request_url);

        for (k, v) in headers {
            request = request.header(k, v);
        }

        let response = request.body(data.to_vec()).send().await?;
        let status = response.status();
        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            return Err(S3Error::UploadFailed(status.as_u16(), body_text));
        }

        let public_url = format!(
            "{}/{}/{}",
            self.config.endpoint.trim_end_matches('/'),
            self.config.bucket,
            key.trim_start_matches('/')
        );
        Ok(public_url)
    }
}
