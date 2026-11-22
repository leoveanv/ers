use ers::book::{Book, BookLocation, BookOpenOptions, ResourceLimits};
use ers::cancellation::Cancellation;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use tempfile::tempdir;

fn read_request_path(stream: &mut TcpStream) -> Option<String> {
    let mut request = Vec::new();
    let mut buffer = [0u8; 512];
    while request.len() < 8192 {
        let length = stream.read(&mut buffer).ok()?;
        if length == 0 {
            return None;
        }
        request.extend_from_slice(&buffer[..length]);
        if request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
            return String::from_utf8_lossy(&request)
                .split_whitespace()
                .nth(1)
                .map(str::to_owned);
        }
    }
    None
}

struct WebServer {
    address: String,
    stopped: Arc<AtomicBool>,
    requests: crossbeam_channel::Receiver<String>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl WebServer {
    fn start() -> Self {
        Self::with_body(
            "<html><head><title>Remote fixture</title></head><body><p>Remote café passage.</p><img src='../image.png'/></body></html>",
        )
    }

    fn with_body(body: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let stopped = Arc::new(AtomicBool::new(false));
        let flag = stopped.clone();
        let (sender, requests) = crossbeam_channel::unbounded();
        let thread = std::thread::spawn(move || {
            while !flag.load(Ordering::SeqCst) {
                let Ok((mut stream, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                };
                // #NOTE: Accepted sockets can inherit nonblocking mode; HTTP headers may also span several reads.
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let Some(path) = read_request_path(&mut stream) else {
                    continue;
                };
                sender.send(path.clone()).unwrap();
                if path == "/slow" {
                    while !flag.load(Ordering::SeqCst) {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    continue;
                }
                let response = match path.as_str() {
                    "/redirect" => "HTTP/1.1 302 Found\r\nLocation: /page/index.html\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
                    "/page/index.html" => { format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()) },
                    "/image.png" => "HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nimage".into(),
                    "/large" => "HTTP/1.1 200 OK\r\nContent-Length: 1000000000\r\nConnection: close\r\n\r\n".into(),
                    _ => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
                };
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Self {
            address,
            stopped,
            requests,
            thread: Some(thread),
        }
    }
    fn url(&self, path: &str) -> BookLocation {
        BookLocation::parse(&format!("{}{path}", self.address)).unwrap()
    }
}
impl Drop for WebServer {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

#[test]
fn redirected_web_pages_resolve_assets_against_the_final_url() {
    let server = WebServer::start();
    let mut book = Book::open(server.url("/redirect")).unwrap_or_else(|error| {
        panic!(
            "{error}; received paths: {:?}",
            server.requests.try_iter().collect::<Vec<_>>()
        )
    });
    let text = book.raw_text(&book.contents()[0].clone()).unwrap();
    assert!(text.contains("Remote café passage."));
    let target = book.resolve_resource("_", "../image.png").unwrap();
    assert_eq!(book.asset(&target.content_id).unwrap().bytes, b"image");
    assert_eq!(book.metadata().title.as_deref(), Some("Remote fixture"));
}

#[test]
fn unsuccessful_http_status_is_not_rendered_as_an_ebook() {
    let server = WebServer::start();
    assert!(Book::open(server.url("/missing")).is_err());
}

#[test]
fn oversized_http_responses_are_rejected_before_body_allocation() {
    let server = WebServer::start();
    assert!(
        Book::open_with_options(
            server.url("/large"),
            BookOpenOptions {
                limits: ResourceLimits {
                    resource_bytes_max: 1024,
                    ..ResourceLimits::default()
                },
                ..BookOpenOptions::default()
            }
        )
        .is_err()
    );
}

#[test]
fn web_loading_can_be_canceled_while_waiting_for_headers() {
    let server = WebServer::start();
    let location = server.url("/slow");
    let cancellation = Cancellation::default();
    let worker_cancel = cancellation.clone();
    let (sender, result) = crossbeam_channel::bounded(1);
    let worker = std::thread::spawn(move || {
        sender
            .send(
                Book::open_with_options(
                    location,
                    BookOpenOptions {
                        cancellation: worker_cancel,
                        ..BookOpenOptions::default()
                    },
                )
                .err(),
            )
            .unwrap();
    });
    assert_eq!(
        server
            .requests
            .recv_timeout(Duration::from_secs(5))
            .unwrap(),
        "/slow"
    );
    cancellation.cancel();
    assert!(matches!(
        result.recv_timeout(Duration::from_secs(2)).unwrap(),
        Some(ers::Error::Canceled)
    ));
    worker.join().unwrap();
}

#[test]
fn web_dump_does_not_create_application_storage() {
    let server = WebServer::start();
    let directory = tempdir().unwrap();
    let config = directory.path().join("configuration");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ers"))
        .arg("--dump")
        .arg(format!("{}/page/index.html", server.address))
        .env("XDG_CONFIG_HOME", &config)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}; received paths: {:?}",
        String::from_utf8_lossy(&output.stderr),
        server.requests.try_iter().collect::<Vec<_>>()
    );
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("Remote café passage.")
    );
    assert!(!config.exists());
}

struct CachedWebFixture {
    directory: tempfile::TempDir,
    location: BookLocation,
    server: Option<WebServer>,
}

impl CachedWebFixture {
    fn new() -> Self {
        Self::with_server(WebServer::start())
    }

    fn with_server(server: WebServer) -> Self {
        Self {
            directory: tempdir().unwrap(),
            location: server.url("/redirect"),
            server: Some(server),
        }
    }

    fn options(&self) -> BookOpenOptions {
        BookOpenOptions {
            web_cache_directory: Some(self.directory.path().join("articles")),
            ..BookOpenOptions::default()
        }
    }

    fn open(&self) -> Book {
        Book::open_with_options(self.location.clone(), self.options()).unwrap()
    }

    fn offline(&mut self) {
        self.server.take();
    }

    fn jobs(&self) -> ers::app::workers::ReaderJobs {
        ers::app::workers::ReaderJobs::start(
            self.location.clone(),
            self.directory.path().join("states.db"),
            40,
        )
    }
}

#[test]
fn cached_article_reopens_offline_with_its_original_text() {
    let mut fixture = CachedWebFixture::new();
    let original = fixture.open().raw_text("_").unwrap();
    fixture.offline();
    assert_eq!(fixture.open().raw_text("_").unwrap(), original);
}

#[test]
fn cached_article_reopening_makes_no_network_requests() {
    let fixture = CachedWebFixture::new();
    fixture.open();
    let server = fixture.server.as_ref().unwrap();
    server.requests.try_iter().for_each(drop);
    fixture.open();
    assert!(server.requests.try_iter().collect::<Vec<_>>().is_empty());
}

#[test]
fn cached_article_prefetches_images_before_they_are_viewed() {
    let mut fixture = CachedWebFixture::new();
    fixture.open();
    fixture.offline();
    let mut book = fixture.open();
    let target = book.resolve_resource("_", "../image.png").unwrap();
    assert_eq!(book.asset(&target.content_id).unwrap().bytes, b"image");
}

#[test]
fn cached_article_fragments_share_the_same_snapshot() {
    let mut fixture = CachedWebFixture::new();
    let original = fixture.open().raw_text("_").unwrap();
    fixture.offline();
    let BookLocation::Web(url) = &mut fixture.location else {
        unreachable!();
    };
    url.set_fragment(Some("section-two"));
    assert_eq!(fixture.open().raw_text("_").unwrap(), original);
}

#[test]
fn cached_article_query_strings_remain_distinct_resources() {
    let mut fixture = CachedWebFixture::new();
    fixture.open();
    fixture.offline();
    let BookLocation::Web(url) = &mut fixture.location else {
        unreachable!();
    };
    url.set_query(Some("edition=other"));
    assert!(Book::open_with_options(fixture.location.clone(), fixture.options()).is_err());
}

#[test]
fn unavailable_images_do_not_prevent_offline_article_reading() {
    let server = WebServer::with_body(
        "<p>Readable article</p><img src='/missing' /><img src='/large' /><img src='file:///etc/passwd' />",
    );
    let mut fixture = CachedWebFixture::with_server(server);
    fixture.open();
    fixture.offline();
    let mut book = fixture.open();
    assert!(book.raw_text("_").unwrap().contains("Readable article"));
    assert!(book.asset("/missing").is_err());
    assert!(book.asset("file:///etc/passwd").is_err());
}

#[test]
fn cached_article_respects_the_current_resource_limit() {
    let mut fixture = CachedWebFixture::new();
    fixture.open();
    fixture.offline();
    let mut options = fixture.options();
    options.limits.resource_bytes_max = 4;
    assert!(Book::open_with_options(fixture.location.clone(), options).is_err());
}

#[test]
fn cached_article_respects_the_current_total_size_limit() {
    let mut fixture = CachedWebFixture::new();
    let length = fixture.open().raw_text("_").unwrap().len();
    fixture.offline();
    let mut options = fixture.options();
    options.limits.archive_bytes_max = length as u64 - 1;
    assert!(Book::open_with_options(fixture.location.clone(), options).is_err());
}

#[test]
fn failed_article_download_does_not_publish_a_snapshot() {
    let mut fixture = CachedWebFixture::new();
    fixture.location = fixture.server.as_ref().unwrap().url("/missing");
    assert!(Book::open_with_options(fixture.location.clone(), fixture.options()).is_err());
    let cache = fixture.directory.path().join("articles");
    assert!(!cache.exists() || std::fs::read_dir(cache).unwrap().next().is_none());
}

#[test]
fn corrupt_article_cache_is_reported_without_refetching() {
    let fixture = CachedWebFixture::new();
    fixture.open();
    let cache = std::fs::read_dir(fixture.directory.path().join("articles"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    std::fs::write(cache, b"corrupt snapshot").unwrap();
    let server = fixture.server.as_ref().unwrap();
    server.requests.try_iter().for_each(drop);
    assert!(Book::open_with_options(fixture.location.clone(), fixture.options()).is_err());
    assert!(server.requests.try_iter().collect::<Vec<_>>().is_empty());
}

fn opened_article(
    jobs: &ers::app::workers::ReaderJobs,
) -> std::sync::Arc<ers::document::RenderedDocument> {
    match jobs.events.recv_timeout(Duration::from_secs(5)).unwrap() {
        ers::app::workers::WorkerEvent::Opened { rendered, .. } => rendered,
        ers::app::workers::WorkerEvent::Failed(message) => panic!("{message}"),
        _ => panic!("Expected the opened article"),
    }
}

fn annotate_article(jobs: &ers::app::workers::ReaderJobs) -> ers::annotations::Annotation {
    use ers::annotations::{AnnotationCommand, AnnotationDraft, AnnotationJob, TextAnchor};
    use ers::app::workers::WorkerEvent;
    let document = opened_article(jobs);
    let text = &document.parsed.source.text;
    let quote = "Remote café passage.";
    let start = text[..text.find(quote).unwrap()].chars().count();
    jobs.annotate(AnnotationJob {
        request_id: 1,
        command: AnnotationCommand::Create(AnnotationDraft {
            anchor: TextAnchor::new("_", start, quote).unwrap(),
            note: "Saved article note".into(),
        }),
    })
    .unwrap();
    loop {
        if let WorkerEvent::Annotation(result) =
            jobs.events.recv_timeout(Duration::from_secs(5)).unwrap()
        {
            assert!(result.error.is_none(), "{:?}", result.error);
            return result.annotations.into_iter().next().unwrap();
        }
    }
}

#[test]
fn article_annotations_survive_an_offline_reader_restart() {
    use ers::app::workers::WorkerEvent;
    let mut fixture = CachedWebFixture::new();
    let jobs = fixture.jobs();
    let annotation = annotate_article(&jobs);
    drop(jobs);
    fixture.offline();
    let jobs = fixture.jobs();
    let WorkerEvent::Opened { info, rendered, .. } =
        jobs.events.recv_timeout(Duration::from_secs(5)).unwrap()
    else {
        panic!("Expected the cached article");
    };
    assert_eq!(info.annotations, vec![annotation.clone()]);
    assert!(annotation.anchor.resolve(&rendered.parsed.source).is_some());
}

#[test]
fn cached_article_annotations_export_to_markdown_offline() {
    use ers::annotations::{LinkCommand, LinkJob, LinkReply};
    use ers::app::workers::WorkerEvent;
    let mut fixture = CachedWebFixture::new();
    let jobs = fixture.jobs();
    annotate_article(&jobs);
    fixture.offline();
    let notes = fixture.directory.path().join("article.md");
    jobs.link(LinkJob {
        request_id: 2,
        command: LinkCommand::Inspect(notes.to_string_lossy().into_owned()),
    })
    .unwrap();
    loop {
        if let WorkerEvent::Link(result) = jobs.events.recv_timeout(Duration::from_secs(5)).unwrap()
        {
            let LinkReply::Destination(destination) = result.reply.unwrap() else {
                panic!("Expected an export destination");
            };
            jobs.link(LinkJob {
                request_id: 3,
                command: LinkCommand::Set {
                    destination,
                    create: true,
                },
            })
            .unwrap();
            break;
        }
    }
    loop {
        match jobs.events.recv_timeout(Duration::from_secs(5)).unwrap() {
            WorkerEvent::Link(result) => {
                result.reply.unwrap();
            }
            WorkerEvent::Export(state) if !state.running && !state.pending() => {
                assert!(state.error.is_none(), "{:?}", state.error);
                break;
            }
            WorkerEvent::Export(state) if state.error.is_some() => panic!("{:?}", state.error),
            _ => {}
        }
    }
    let exported = std::fs::read_to_string(notes).unwrap();
    assert!(exported.contains("Remote café passage."));
    assert!(exported.contains("Saved article note"));
}

#[test]
fn canceling_image_prefetch_does_not_publish_a_partial_article() {
    let server = WebServer::with_body("<p>Article</p><img src='/slow' />");
    let fixture = CachedWebFixture::with_server(server);
    let mut options = fixture.options();
    let cancellation = Cancellation::default();
    options.cancellation = cancellation.clone();
    let location = fixture.location.clone();
    let worker = std::thread::spawn(move || Book::open_with_options(location, options));
    let server = fixture.server.as_ref().unwrap();
    loop {
        if server
            .requests
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            == "/slow"
        {
            break;
        }
    }
    cancellation.cancel();
    assert!(matches!(worker.join().unwrap(), Err(ers::Error::Canceled)));
    let cache = fixture.directory.path().join("articles");
    assert!(!cache.exists() || std::fs::read_dir(cache).unwrap().next().is_none());
}

#[test]
fn identical_articles_at_different_urls_keep_separate_annotations() {
    use ers::app::workers::WorkerEvent;
    let mut fixture = CachedWebFixture::new();
    let jobs = fixture.jobs();
    annotate_article(&jobs);
    drop(jobs);
    fixture.location = fixture.server.as_ref().unwrap().url("/page/index.html");
    let jobs = fixture.jobs();
    let WorkerEvent::Opened { info, .. } =
        jobs.events.recv_timeout(Duration::from_secs(5)).unwrap()
    else {
        panic!("Expected the other article");
    };
    assert!(info.annotations.is_empty());
}

#[test]
fn reader_image_preview_uses_the_offline_article_cache() {
    use ers::app::modes::images::ImageMode;
    use ers::app::runtime::{ReaderLaunch, ReaderRuntime};
    use ers::app::{ReaderMode, Viewport};
    use ers::configuration::{Configuration, InputKey};
    let mut fixture = CachedWebFixture::new();
    let mut runtime = ReaderRuntime::start(ReaderLaunch {
        image_decoder: env!("CARGO_BIN_EXE_ers").into(),
        executables: ers::integrations::ExecutableSearch::capture().unwrap(),
        location: fixture.location.clone(),
        database: fixture.directory.path().join("states.db"),
        configuration: Configuration::default(),
        viewport: Viewport {
            columns: 80,
            rows: 24,
        },
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while runtime.model.is_none() {
        runtime.poll().unwrap();
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    fixture.offline();
    runtime.handle_key(InputKey::Character('o'));
    runtime.handle_key(InputKey::Enter);
    loop {
        runtime.poll().unwrap();
        if let ReaderMode::Images(mode) = runtime.model.as_ref().unwrap().view().mode {
            match mode.as_ref() {
                ImageMode::Preview(preview) => {
                    assert_eq!(preview.payload.bytes, b"image");
                    break;
                }
                ImageMode::Failed(message) => panic!("{message}"),
                _ => {}
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Image preview did not open"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn image_worker_reads_prefetched_article_images_offline() {
    use ers::images::{ImageJob, ImageTarget, ImageWorker, ImageWorkerOptions};
    let mut fixture = CachedWebFixture::new();
    fixture.open();
    fixture.offline();
    let mut options = ImageWorkerOptions::new(env!("CARGO_BIN_EXE_ers").into());
    options.web_cache_directory = fixture.options().web_cache_directory;
    let worker = ImageWorker::start(options);
    worker.request(
        fixture.location.clone(),
        ImageJob {
            request_id: 1,
            target: ImageTarget {
                content_id: "_".into(),
                path: "../image.png".into(),
                row_offset: 0,
            },
        },
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(result) = worker.take_result() {
            assert_eq!(result.result.unwrap().bytes, b"image");
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Image worker did not finish"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn parallel_extract_fetches_markdown_and_parses_headings_links_and_images() {
    use ers::book::ParallelExtractConfig;
    use ers::document::layout::{LayoutOptions, layout_document};
    use ers::document::text::parse_document;
    use url::Url;

    let image_server = WebServer::start();
    let image_url = format!("{}/image.png", image_server.address);
    let markdown = format!(
        "#### 3\\.B\\.2 Long heading from Parallel Extract that wraps cleanly\n\n\
         []({image_url})\n\n\
         Read the [original report](https://example.com/report).\n\n\
         | Metric | Value |\n| --- | --- |\n| Latency | 0.58s |\n"
    );
    let response_json = serde_json::json!({
        "extract_id": "extract_test",
        "results": [{
            "url": "https://example.com/article/",
            "title": "Parallel Markdown Article",
            "excerpts": [],
            "full_content": markdown
        }],
        "errors": []
    })
    .to_string();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1/extract", listener.local_addr().unwrap());
    let (tx, rx) = crossbeam_channel::bounded(1);
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0u8; 1024];
        loop {
            let n = stream.read(&mut buffer).unwrap();
            if n == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..n]);
            if let Some(pos) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                let header_text = String::from_utf8_lossy(&request[..pos]);
                let content_len = header_text
                    .lines()
                    .find_map(|line| {
                        let (k, v) = line.split_once(':')?;
                        k.eq_ignore_ascii_case("content-length")
                            .then(|| v.trim().parse::<usize>().ok())
                            .flatten()
                    })
                    .unwrap_or(0);
                if request.len() >= pos + 4 + content_len {
                    break;
                }
            }
        }
        tx.send(String::from_utf8_lossy(&request).into_owned())
            .unwrap();
        let reply = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_json}",
            response_json.len()
        );
        stream.write_all(reply.as_bytes()).unwrap();
    });

    let cache_dir = tempdir().unwrap();
    let mut book = Book::open_with_options(
        BookLocation::parse("https://example.com/article/").unwrap(),
        BookOpenOptions {
            web_cache_directory: Some(cache_dir.path().to_path_buf()),
            parallel_extract: Some(ParallelExtractConfig {
                api_key: "test-parallel-key".into(),
                endpoint: Url::parse(&endpoint).unwrap(),
            }),
            ..BookOpenOptions::default()
        },
    )
    .unwrap();
    server.join().unwrap();

    let captured = rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(captured.starts_with("POST /v1/extract HTTP/1.1"));
    assert!(
        captured
            .to_ascii_lowercase()
            .contains("x-api-key: test-parallel-key")
    );
    assert!(captured.contains("\"full_content\":true"));

    assert_eq!(
        book.metadata().title.as_deref(),
        Some("Parallel Markdown Article")
    );
    let html = book.raw_text("_").unwrap();
    let parsed = parse_document("_", &html).unwrap();
    assert!(
        parsed
            .paragraphs
            .iter()
            .any(|p| p.text == "3.B.2 Long heading from Parallel Extract that wraps cleanly")
    );
    assert!(
        parsed
            .semantics
            .links
            .iter()
            .any(|link| link.href == "https://example.com/report")
    );
    assert_eq!(book.asset(&image_url).unwrap().bytes, b"image");

    let layout = layout_document(&parsed, LayoutOptions::reader(40, false));
    assert!(
        layout
            .text_lines
            .iter()
            .all(|line| ers::document::text::cell_width(line) <= 40)
    );
}

