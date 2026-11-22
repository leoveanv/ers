mod cache;
mod markdown;
mod parallel;

pub use parallel::ParallelExtractConfig;

use super::BookMetadata;
use super::BookNavigation;
use super::location::{validate_web_url, web_resource_url};
use super::{BookAsset, BookBackend, BookDescriptor, BookOpenOptions, ResourceTarget};
use crate::cancellation::Cancellation;
use crate::document::html::{HtmlToken, HtmlTokenizer, unescape_html};
use crate::{Error, Result};
use cache::{ArticleCache, CachedPage, IMAGE_COUNT_MAX, SNAPSHOT_BYTES_MAX};
use reqwest::{Client, Url};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::runtime::{Builder, Runtime};

pub(super) struct WebBook {
    descriptor: BookDescriptor,
    html: String,
    base: Url,
    client: Client,
    cancellation: Cancellation,
    resource_bytes_max: u64,
    cache: Option<ArticleCache>,
}

#[derive(Clone, Copy)]
struct FetchLimits {
    bytes_max: u64,
    timeout: Duration,
}

struct WebResponse {
    url: Url,
    content_type: String,
    bytes: Vec<u8>,
}

fn runtime() -> Result<&'static Runtime> {
    static RUNTIME: OnceLock<std::result::Result<Runtime, std::io::Error>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            Builder::new_multi_thread()
                .worker_threads(2)
                .max_blocking_threads(4)
                .enable_all()
                .build()
        })
        .as_ref()
        .map_err(|error| Error::Compatibility(format!("Cannot start the network runtime: {error}")))
}

fn network_error(error: reqwest::Error) -> Error {
    Error::Compatibility(format!("Network request failed: {error}"))
}

async fn canceled(cancellation: &Cancellation) {
    while !cancellation.is_canceled() {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn request(client: &Client, url: &Url, limits: FetchLimits) -> Result<WebResponse> {
    validate_web_url(url)?;
    let mut response = client
        .get(url.clone())
        .timeout(limits.timeout)
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
            "The web response exceeds its size limit.".into(),
        ));
    }
    let url = response.url().clone();
    validate_web_url(&url)?;
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(network_error)? {
        if (bytes.len() as u64).saturating_add(chunk.len() as u64) > limits.bytes_max {
            return Err(Error::Compatibility(
                "The web response exceeds its decoded size limit.".into(),
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(WebResponse {
        url,
        content_type,
        bytes,
    })
}

fn fetch(
    client: &Client,
    url: &Url,
    limits: FetchLimits,
    cancellation: &Cancellation,
) -> Result<WebResponse> {
    cancellation.check()?;
    runtime()?.block_on(async { tokio::select! { biased; _ = canceled(cancellation) => Err(Error::Canceled), result = request(client, url, limits) => result } })
}

fn decode_html(response: &WebResponse) -> Result<String> {
    let label = response
        .content_type
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix("charset="))
        .map(|label| label.trim_matches(['\'', '"']));
    let (encoding, skip) = encoding_rs::Encoding::for_bom(&response.bytes).unwrap_or((
        label
            .and_then(|label| encoding_rs::Encoding::for_label(label.as_bytes()))
            .unwrap_or(encoding_rs::UTF_8),
        0,
    ));
    let (text, errors) = encoding.decode_without_bom_handling(&response.bytes[skip..]);
    if errors {
        return Err(Error::Compatibility(
            "The web document contains invalid text for its character encoding.".into(),
        ));
    }
    Ok(text.into_owned())
}

fn title(html: &str) -> Option<String> {
    let mut active = false;
    let mut title = String::new();
    for token in HtmlTokenizer::new(html) {
        match token {
            HtmlToken::Start(tag) if tag.name == "title" => active = true,
            HtmlToken::End(tag) if tag == "title" => break,
            HtmlToken::Data(data) if active => title.push_str(&unescape_html(&data)),
            _ => {}
        }
    }
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    (!title.is_empty()).then_some(title)
}

impl WebBook {
    pub fn open(url: &Url, options: &BookOpenOptions) -> Result<Self> {
        validate_web_url(url)?;
        let client = web_client()?;
        let cache = options
            .web_cache_directory
            .as_ref()
            .map(|directory| ArticleCache::new(directory, url, options.limits));
        let page = match cache
            .as_ref()
            .map(ArticleCache::load)
            .transpose()?
            .flatten()
        {
            Some(page) => page,
            None => import_page(&client, url, cache.as_ref(), options)?,
        };
        let html = page.html;
        let descriptor = BookDescriptor {
            metadata: BookMetadata {
                title: title(&html),
                source: Some(web_resource_url(url).to_string()),
                format: Some("HTML".into()),
                ..BookMetadata::default()
            },
            contents: vec!["_".into()],
            spine_len: 1,
            toc: Vec::new(),
            navigation: BookNavigation::default(),
        };
        Ok(Self {
            descriptor,
            html,
            base: page.base,
            client,
            cancellation: options.cancellation.clone(),
            resource_bytes_max: options.limits.resource_bytes_max,
            cache,
        })
    }
}

fn web_client() -> Result<Client> {
    let _guard = runtime()?.enter();
    Client::builder()
        .user_agent(concat!("ers/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 10 || validate_web_url(attempt.url()).is_err() {
                attempt.error("Unsafe URL or too many redirects")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(network_error)
}

fn fetch_page_via_parallel(
    client: &Client,
    config: &ParallelExtractConfig,
    url: &Url,
    options: &BookOpenOptions,
) -> Result<CachedPage> {
    options.cancellation.check()?;
    let limits = FetchLimits {
        bytes_max: options.limits.resource_bytes_max,
        timeout: Duration::from_secs(30),
    };
    let extracted = runtime()?.block_on(async {
        tokio::select! {
            biased;
            _ = canceled(&options.cancellation) => Err(Error::Canceled),
            result = parallel::extract_markdown(client, config, url, limits) => result,
        }
    })?;
    Ok(CachedPage {
        html: markdown::markdown_to_html(&extracted.markdown, extracted.title.as_deref()),
        base: web_resource_url(&extracted.url),
    })
}

fn fetch_page_directly(
    client: &Client,
    url: &Url,
    options: &BookOpenOptions,
) -> Result<CachedPage> {
    let response = fetch(
        client,
        url,
        FetchLimits {
            bytes_max: options.limits.resource_bytes_max,
            timeout: Duration::from_secs(30),
        },
        &options.cancellation,
    )?;
    Ok(CachedPage {
        html: decode_html(&response)?,
        base: web_resource_url(&response.url),
    })
}

fn import_page(
    client: &Client,
    url: &Url,
    cache: Option<&ArticleCache>,
    options: &BookOpenOptions,
) -> Result<CachedPage> {
    let page = match options
        .parallel_extract
        .as_ref()
        .filter(|config| parallel::should_use_parallel(config, url))
    {
        Some(config) => fetch_page_via_parallel(client, config, url, options)?,
        None => fetch_page_directly(client, url, options)?,
    };
    if page.html.len() as u64 > options.limits.resource_bytes_max {
        return Err(Error::Compatibility(
            "The decoded web document exceeds its size limit.".into(),
        ));
    }
    let Some(cache) = cache else {
        return Ok(page);
    };
    let assets = prefetch_images(client, &page, options)?;
    cache.save(&page, &assets, &options.cancellation)
}

const IMAGE_PREFETCH_CONCURRENCY: usize = 8;

fn prefetch_images(
    client: &Client,
    page: &CachedPage,
    options: &BookOpenOptions,
) -> Result<BTreeMap<String, BookAsset>> {
    let maximum = options.limits.archive_bytes_max.min(SNAPSHOT_BYTES_MAX);
    let mut remaining = maximum
        .checked_sub(page.html.len() as u64)
        .ok_or_else(|| Error::Compatibility("The article exceeds its cache size limit.".into()))?;
    if options.limits.archive_entries_max < 2 {
        return Err(Error::Compatibility(
            "The article exceeds its cache entry limit.".into(),
        ));
    }
    let document = crate::document::text::parse_document("_", &page.html)?;
    let max_images = IMAGE_COUNT_MAX.min(options.limits.archive_entries_max - 2);
    let mut seen = BTreeSet::new();
    let mut urls = Vec::new();
    for path in document
        .paragraphs
        .iter()
        .filter_map(|paragraph| paragraph.image_path.as_deref())
    {
        if seen.len() >= max_images {
            break;
        }
        let Ok(url) = image_url(&page.base, path) else {
            continue;
        };
        if seen.insert(url.to_string()) {
            urls.push(url);
        }
    }
    if urls.is_empty() || remaining == 0 {
        return Ok(BTreeMap::new());
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    let cancellation = &options.cancellation;
    let resource_bytes_max = options.limits.resource_bytes_max;
    runtime()?.block_on(async {
        let mut queue = urls.into_iter();
        let mut pending = tokio::task::JoinSet::new();
        let mut assets = BTreeMap::new();
        loop {
            cancellation.check()?;
            while pending.len() < IMAGE_PREFETCH_CONCURRENCY && remaining > 0 {
                let Some(url) = queue.next() else {
                    break;
                };
                let timeout = deadline.saturating_duration_since(Instant::now());
                if timeout.is_zero() {
                    break;
                }
                let client = client.clone();
                let limits = FetchLimits {
                    bytes_max: resource_bytes_max.min(remaining),
                    timeout,
                };
                pending.spawn(async move {
                    let key = url.to_string();
                    let result = request_asset(&client, &url, limits).await;
                    (key, result)
                });
            }
            if pending.is_empty() {
                break;
            }
            let joined = tokio::select! {
                biased;
                _ = canceled(cancellation) => {
                    pending.abort_all();
                    return Err(Error::Canceled);
                }
                next = pending.join_next() => next,
            };
            let Some(Ok((key, result))) = joined else {
                continue;
            };
            if let Ok(asset) = result
                && asset.bytes.len() as u64 <= remaining
            {
                remaining -= asset.bytes.len() as u64;
                assets.insert(key, asset);
            }
        }
        Ok(assets)
    })
}

async fn request_asset(client: &Client, url: &Url, limits: FetchLimits) -> Result<BookAsset> {
    let response = request(client, url, limits).await?;
    let name = response
        .url
        .path_segments()
        .and_then(Iterator::last)
        .filter(|name| !name.is_empty())
        .unwrap_or("image")
        .to_owned();
    Ok(BookAsset {
        name,
        bytes: response.bytes,
    })
}

fn image_url(base: &Url, path: &str) -> Result<Url> {
    let url = base
        .join(path)
        .map_err(|error| Error::Compatibility(format!("Invalid web resource URL: {error}")))?;
    validate_web_url(&url)?;
    Ok(web_resource_url(&url))
}

fn fetch_asset(
    client: &Client,
    url: &Url,
    limits: FetchLimits,
    cancellation: &Cancellation,
) -> Result<BookAsset> {
    let response = fetch(client, url, limits, cancellation)?;
    let name = response
        .url
        .path_segments()
        .and_then(Iterator::last)
        .filter(|name| !name.is_empty())
        .unwrap_or("image")
        .to_owned();
    Ok(BookAsset {
        name,
        bytes: response.bytes,
    })
}

impl BookBackend for WebBook {
    fn descriptor(&self) -> &BookDescriptor {
        &self.descriptor
    }
    fn raw_text(&mut self, id: &str) -> Result<String> {
        if id != "_" {
            return Err(Error::Compatibility(
                "The web document does not exist.".into(),
            ));
        }
        Ok(self.html.clone())
    }
    fn asset(&mut self, id: &str) -> Result<BookAsset> {
        let url = image_url(&self.base, id)?;
        if let Some(cache) = &self.cache {
            return cache.asset(&url);
        }
        fetch_asset(
            &self.client,
            &url,
            FetchLimits {
                bytes_max: self.resource_bytes_max,
                timeout: Duration::from_secs(30),
            },
            &self.cancellation,
        )
    }
    fn resolve_resource(&self, _: &str, href: &str) -> Result<ResourceTarget> {
        let mut url = self
            .base
            .join(href)
            .map_err(|error| Error::Compatibility(format!("Invalid web resource URL: {error}")))?;
        validate_web_url(&url)?;
        let fragment = url.fragment().map(|value| {
            percent_encoding::percent_decode_str(value)
                .decode_utf8_lossy()
                .into_owned()
        });
        url.set_fragment(None);
        let mut base = self.base.clone();
        base.set_fragment(None);
        Ok(ResourceTarget {
            content_id: if url == base {
                "_".into()
            } else {
                url.to_string()
            },
            fragment,
        })
    }
}
