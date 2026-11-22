use super::super::location::validate_web_url;
use super::{FetchLimits, network_error};
use crate::{Error, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::path::Path;
use url::{Host, Url};

const DEFAULT_ENDPOINT: &str = "https://api.parallel.ai/v1/extract";
const API_KEY_ENV: &str = "PARALLEL_API_KEY";

#[derive(Clone, Debug)]
pub struct ParallelExtractConfig {
    pub api_key: String,
    pub endpoint: Url,
}

impl ParallelExtractConfig {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            endpoint: Url::parse(DEFAULT_ENDPOINT).expect("The default Parallel endpoint is valid"),
        }
    }

    pub fn from_env() -> Option<Self> {
        let key = std::env::var(API_KEY_ENV)
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .or_else(|| read_dotenv_key(Path::new(".env"), API_KEY_ENV))?;
        Some(Self::new(key))
    }
}

pub(super) struct ExtractedArticle {
    pub url: Url,
    pub title: Option<String>,
    pub markdown: String,
}

#[derive(Serialize)]
struct ExtractRequest<'a> {
    urls: [&'a str; 1],
    advanced_settings: AdvancedExtractSettings,
}

#[derive(Serialize)]
struct AdvancedExtractSettings {
    full_content: bool,
}

#[derive(Deserialize)]
struct ExtractResponse {
    #[serde(default)]
    results: Vec<ExtractResult>,
    #[serde(default)]
    errors: Vec<ExtractError>,
}

#[derive(Deserialize)]
struct ExtractResult {
    url: String,
    title: Option<String>,
    full_content: Option<String>,
}

#[derive(Deserialize)]
struct ExtractError {
    error_type: Option<String>,
    http_status_code: Option<u16>,
    content: Option<String>,
}

// #NOTE: Never send loopback URLs to Parallel's cloud endpoint; allow them only when targeting a custom endpoint.
pub(super) fn should_use_parallel(config: &ParallelExtractConfig, url: &Url) -> bool {
    !(config.endpoint.as_str() == DEFAULT_ENDPOINT && is_loopback_url(url))
}

fn is_loopback_url(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(address)) => IpAddr::V4(address).is_loopback(),
        Some(Host::Ipv6(address)) => IpAddr::V6(address).is_loopback(),
        None => false,
    }
}

pub(super) async fn extract_markdown(
    client: &Client,
    config: &ParallelExtractConfig,
    url: &Url,
    limits: FetchLimits,
) -> Result<ExtractedArticle> {
    validate_web_url(url)?;
    validate_web_url(&config.endpoint)?;
    let body = serde_json::to_vec(&ExtractRequest {
        urls: [url.as_str()],
        advanced_settings: AdvancedExtractSettings { full_content: true },
    })?;
    let mut response = client
        .post(config.endpoint.clone())
        .timeout(limits.timeout)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header("x-api-key", &config.api_key)
        .body(body)
        .send()
        .await
        .map_err(network_error)?
        .error_for_status()
        .map_err(network_error)?;
    if response
        .content_length()
        .is_some_and(|length| length > limits.bytes_max)
    {
        return Err(Error::Compatibility(
            "The Parallel extract response exceeds its size limit.".into(),
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(network_error)? {
        if (bytes.len() as u64).saturating_add(chunk.len() as u64) > limits.bytes_max {
            return Err(Error::Compatibility(
                "The Parallel extract response exceeds its decoded size limit.".into(),
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    let parsed: ExtractResponse = serde_json::from_slice(&bytes).map_err(|error| {
        Error::Compatibility(format!("Invalid Parallel extract response: {error}"))
    })?;
    if let Some(result) = parsed.results.into_iter().next() {
        let markdown = result
            .full_content
            .filter(|content| !content.trim().is_empty())
            .ok_or_else(|| {
                Error::Compatibility("Parallel extract returned empty full content.".into())
            })?;
        let final_url = Url::parse(&result.url)
            .ok()
            .filter(|candidate| validate_web_url(candidate).is_ok())
            .unwrap_or_else(|| url.clone());
        return Ok(ExtractedArticle {
            url: final_url,
            title: result.title,
            markdown,
        });
    }
    if let Some(error) = parsed.errors.into_iter().next() {
        let kind = error.error_type.unwrap_or_else(|| "extract_error".into());
        let status = error
            .http_status_code
            .map(|code| format!(" (HTTP {code})"))
            .unwrap_or_default();
        let detail = error.content.unwrap_or_default();
        return Err(Error::Compatibility(format!(
            "Parallel extract failed ({kind}{status}): {detail}"
        )));
    }
    Err(Error::Compatibility(
        "Parallel extract returned no results.".into(),
    ))
}

fn read_dotenv_key(path: &Path, target: &str) -> Option<String> {
    let contents = std::fs::read_to_string(path).ok()?;
    parse_dotenv_key(&contents, target)
}

fn parse_dotenv_key(contents: &str, target: &str) -> Option<String> {
    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").map_or(line, str::trim_start);
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != target {
            continue;
        }
        let value = value.trim();
        let unquoted = if (value.starts_with('"') && value.ends_with('"') && value.len() >= 2)
            || (value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2)
        {
            &value[1..value.len() - 1]
        } else {
            value
        };
        let cleaned = unquoted.trim();
        if !cleaned.is_empty() {
            return Some(cleaned.to_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{ParallelExtractConfig, parse_dotenv_key, should_use_parallel};
    use url::Url;

    #[test]
    fn parses_dotenv_values_with_quotes_and_export() {
        let sample = "# comment\nexport PARALLEL_API_KEY=\"test-key-123\"\nOTHER=1\n";
        assert_eq!(
            parse_dotenv_key(sample, "PARALLEL_API_KEY").as_deref(),
            Some("test-key-123")
        );
        assert_eq!(
            parse_dotenv_key("PARALLEL_API_KEY='single'", "PARALLEL_API_KEY").as_deref(),
            Some("single")
        );
        assert_eq!(
            parse_dotenv_key("PARALLEL_API_KEY=   ", "PARALLEL_API_KEY"),
            None
        );
    }

    #[test]
    fn skips_loopback_urls_for_default_cloud_endpoint() {
        let config = ParallelExtractConfig::new("secret");
        assert!(!should_use_parallel(
            &config,
            &Url::parse("http://127.0.0.1:8080/page").unwrap()
        ));
        assert!(!should_use_parallel(
            &config,
            &Url::parse("http://localhost:3000/article").unwrap()
        ));
        assert!(should_use_parallel(
            &config,
            &Url::parse("https://metr.org/blog/2026-11-20-q4-2026-capability-update/").unwrap()
        ));
    }
}
