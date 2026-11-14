use super::super::location::{validate_web_url, web_resource_url};
use super::super::{BookAsset, ResourceLimits};
use crate::cancellation::Cancellation;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use url::Url;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

pub(super) const IMAGE_COUNT_MAX: usize = 128;
pub(super) const SNAPSHOT_BYTES_MAX: u64 = 128 * 1024 * 1024;
const METADATA_BYTES_MAX: u64 = 1024 * 1024;

pub(super) struct CachedPage {
    pub html: String,
    pub base: Url,
}

#[derive(Serialize, Deserialize)]
struct SnapshotMetadata {
    version: u32,
    source: String,
    base: String,
    assets: BTreeMap<String, String>,
}

pub(super) struct ArticleCache {
    path: PathBuf,
    source: Url,
    limits: ResourceLimits,
}

impl ArticleCache {
    pub fn new(directory: &Path, source: &Url, limits: ResourceLimits) -> Self {
        let source = web_resource_url(source);
        Self {
            path: directory.join(format!("{}.zip", digest(source.as_str()))),
            source,
            limits,
        }
    }

    pub fn load(&self) -> Result<Option<CachedPage>> {
        match File::open(&self.path) {
            Ok(file) => self.read_page(file).map(Some).map_err(|error| {
                Error::Compatibility(format!(
                    "Cannot read cached article {}: {error}. The saved snapshot was not refreshed.",
                    self.path.display()
                ))
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn archive(&self, file: File) -> Result<ZipArchive<File>> {
        let maximum = self.limits.archive_bytes_max.min(SNAPSHOT_BYTES_MAX);
        if file.metadata()?.len() > maximum.saturating_add(METADATA_BYTES_MAX * 2) {
            return Err(cache_limit());
        }
        let mut archive = ZipArchive::new(file)?;
        if archive.len() > (IMAGE_COUNT_MAX + 2).min(self.limits.archive_entries_max) {
            return Err(cache_limit());
        }
        let mut total = 0u64;
        for index in 0..archive.len() {
            let entry = archive.by_index(index)?;
            if entry.name()? == "metadata.json" {
                if entry.size() > METADATA_BYTES_MAX {
                    return Err(cache_limit());
                }
            } else {
                total = total.saturating_add(entry.size());
            }
            if total > maximum {
                return Err(cache_limit());
            }
        }
        Ok(archive)
    }

    fn metadata(&self, archive: &mut ZipArchive<File>) -> Result<SnapshotMetadata> {
        let bytes = read_entry(archive, "metadata.json", METADATA_BYTES_MAX)?;
        let metadata = serde_json::from_slice::<SnapshotMetadata>(&bytes)?;
        if metadata.version != 1 || metadata.source != self.source.as_str() {
            return Err(Error::Compatibility(
                "The cached article has an unsupported version or a different URL.".into(),
            ));
        }
        Ok(metadata)
    }

    fn read_page(&self, file: File) -> Result<CachedPage> {
        let mut archive = self.archive(file)?;
        let metadata = self.metadata(&mut archive)?;
        let base = Url::parse(&metadata.base).map_err(|error| {
            Error::Compatibility(format!("Invalid cached article URL: {error}"))
        })?;
        validate_web_url(&base)?;
        let html = read_entry(&mut archive, "article.html", self.limits.resource_bytes_max)?;
        let html = String::from_utf8(html).map_err(|error| {
            Error::Compatibility(format!("Invalid cached article text: {error}"))
        })?;
        Ok(CachedPage { html, base })
    }

    pub fn asset(&self, url: &Url) -> Result<BookAsset> {
        let mut archive = self.archive(File::open(&self.path)?)?;
        let metadata = self.metadata(&mut archive)?;
        let name = metadata.assets.get(url.as_str()).ok_or_else(|| {
            Error::Compatibility(
                "This image was unavailable or exceeded the article cache limits when saved."
                    .into(),
            )
        })?;
        let bytes = read_entry(
            &mut archive,
            &format!("images/{}", digest(url.as_str())),
            self.limits.resource_bytes_max,
        )?;
        Ok(BookAsset {
            name: name.clone(),
            bytes,
        })
    }

    // #NOTE: Concurrent imports keep the first complete snapshot, including its annotation positions.
    pub fn save(
        &self,
        page: &CachedPage,
        assets: &BTreeMap<String, BookAsset>,
        cancellation: &Cancellation,
    ) -> Result<CachedPage> {
        cancellation.check()?;
        let parent = self.path.parent().ok_or_else(|| {
            Error::Compatibility("The article cache needs a parent directory.".into())
        })?;
        std::fs::create_dir_all(parent)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        self.write_snapshot(temporary.as_file_mut(), page, assets, cancellation)?;
        temporary.as_file().sync_all()?;
        cancellation.check()?;
        match temporary.persist_noclobber(&self.path) {
            Ok(_) => crate::filesystem::sync_parent(&self.path)?,
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.error.into()),
        }
        self.load()?
            .ok_or_else(|| Error::Compatibility("The saved article disappeared.".into()))
    }

    fn write_snapshot(
        &self,
        file: &mut File,
        page: &CachedPage,
        assets: &BTreeMap<String, BookAsset>,
        cancellation: &Cancellation,
    ) -> Result<()> {
        let metadata = SnapshotMetadata {
            version: 1,
            source: self.source.to_string(),
            base: page.base.to_string(),
            assets: assets
                .iter()
                .map(|(url, asset)| (url.clone(), asset.name.clone()))
                .collect(),
        };
        let metadata = serde_json::to_vec(&metadata)?;
        if metadata.len() as u64 > METADATA_BYTES_MAX {
            return Err(cache_limit());
        }
        let mut archive = ZipWriter::new(file);
        write_entry(&mut archive, "metadata.json", &metadata)?;
        write_entry(&mut archive, "article.html", page.html.as_bytes())?;
        for (url, asset) in assets {
            cancellation.check()?;
            write_entry(
                &mut archive,
                &format!("images/{}", digest(url)),
                &asset.bytes,
            )?;
        }
        archive.finish()?;
        Ok(())
    }
}

fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn read_entry(archive: &mut ZipArchive<File>, name: &str, maximum: u64) -> Result<Vec<u8>> {
    let entry = archive.by_name(name)?;
    if entry.size() > maximum {
        return Err(cache_limit());
    }
    let mut bytes = Vec::new();
    entry
        .take(maximum.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        return Err(cache_limit());
    }
    Ok(bytes)
}

fn write_entry(archive: &mut ZipWriter<&mut File>, name: &str, bytes: &[u8]) -> Result<()> {
    archive.start_file(
        name,
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
    )?;
    archive.write_all(bytes)?;
    Ok(())
}

fn cache_limit() -> Error {
    Error::Compatibility("The cached article exceeds its resource limits.".into())
}
