use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine;
use image::GenericImageView;
use rmcp::model::{Annotations, CallToolResult, ContentBlock, TextContent};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::edit::resolve_path;

const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

fn visible_text(text: impl Into<String>) -> ContentBlock {
    ContentBlock::Text(
        TextContent::new(text).with_annotations(Annotations::default().with_priority(0.0)),
    )
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ImageReadParams {
    /// Local file path or http(s) URL of the image to load.
    pub source: String,
    /// Optional crop rectangle in pixels. Coordinates are measured from the top-left corner.
    /// use to zoom in and get more details.
    #[serde(default)]
    pub crop: Option<CropParams>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CropParams {
    /// Left edge of the crop rectangle in pixels.
    pub x: u32,
    /// Top edge of the crop rectangle in pixels.
    pub y: u32,
    /// Width of the crop rectangle in pixels.
    pub width: u32,
    /// Height of the crop rectangle in pixels.
    pub height: u32,
}

pub struct ImageTool;

impl ImageTool {
    pub fn new() -> Self {
        Self
    }

    pub async fn image_read_with_cwd(
        &self,
        params: ImageReadParams,
        working_dir: Option<&Path>,
    ) -> CallToolResult {
        match load_image(&params, working_dir).await {
            Ok(loaded) => {
                let mut result = CallToolResult::success(vec![
                    visible_text(loaded.summary(&params.source)),
                    ContentBlock::image(loaded.data, loaded.mime_type.clone()),
                ]);
                result.structured_content = Some(json!({
                    "source": params.source,
                    "mimeType": loaded.mime_type,
                    "width": loaded.width,
                    "height": loaded.height,
                    "bytes": loaded.bytes_len,
                    "originalWidth": loaded.original_width,
                    "originalHeight": loaded.original_height,
                    "crop": params.crop,
                }));
                result
            }
            Err(error) => CallToolResult::error(vec![visible_text(format!("Error: {error}"))]),
        }
    }
}

impl Default for ImageTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
struct LoadedImage {
    data: String,
    mime_type: String,
    bytes_len: usize,
    width: u32,
    height: u32,
    original_width: u32,
    original_height: u32,
    cropped: bool,
}

impl LoadedImage {
    fn summary(&self, source: &str) -> String {
        let crop_note = if self.cropped {
            format!(
                " Cropped from {}x{} to {}x{}.",
                self.original_width, self.original_height, self.width, self.height
            )
        } else {
            String::new()
        };

        format!(
            "Loaded image from {source} ({} bytes, {}, {}x{}).{crop_note}",
            self.bytes_len, self.mime_type, self.width, self.height
        )
    }
}

async fn load_image(
    params: &ImageReadParams,
    working_dir: Option<&Path>,
) -> Result<LoadedImage, String> {
    if params.source.trim().is_empty() {
        return Err("source cannot be empty".to_string());
    }

    let bytes = load_image_bytes(&params.source, working_dir).await?;
    ensure_image_size(bytes.len() as u64)?;

    let format = image::guess_format(&bytes).map_err(|_| {
        "unsupported image format; supported formats are png, jpeg, gif, and webp".to_string()
    })?;
    let mime_type = mime_type(format)?;
    let image = image::load_from_memory_with_format(&bytes, format)
        .map_err(|error| format!("failed to decode image: {error}"))?;
    let (original_width, original_height) = image.dimensions();

    let Some(crop) = &params.crop else {
        return Ok(LoadedImage {
            data: base64::prelude::BASE64_STANDARD.encode(&bytes),
            mime_type: mime_type.to_string(),
            bytes_len: bytes.len(),
            width: original_width,
            height: original_height,
            original_width,
            original_height,
            cropped: false,
        });
    };

    validate_crop(crop, original_width, original_height)?;
    let cropped = image.crop_imm(crop.x, crop.y, crop.width, crop.height);
    let mut cropped_bytes = Cursor::new(Vec::new());
    cropped
        .write_to(&mut cropped_bytes, image::ImageFormat::Png)
        .map_err(|error| format!("failed to encode cropped image: {error}"))?;
    let cropped_bytes = cropped_bytes.into_inner();
    ensure_image_size(cropped_bytes.len() as u64)?;

    Ok(LoadedImage {
        data: base64::prelude::BASE64_STANDARD.encode(&cropped_bytes),
        mime_type: "image/png".to_string(),
        bytes_len: cropped_bytes.len(),
        width: crop.width,
        height: crop.height,
        original_width,
        original_height,
        cropped: true,
    })
}

async fn load_image_bytes(source: &str, working_dir: Option<&Path>) -> Result<Vec<u8>, String> {
    #[cfg(windows)]
    validate_windows_image_path(Path::new(source))?;

    if let Ok(url) = url::Url::parse(source) {
        match url.scheme() {
            "http" | "https" => load_url_bytes(url).await,
            "file" => {
                let path = url
                    .to_file_path()
                    .map_err(|_| "invalid file URL".to_string())?;
                load_file_bytes(path)
            }
            _ => load_file_bytes(resolve_path(source, working_dir)),
        }
    } else {
        load_file_bytes(resolve_path(source, working_dir))
    }
}

async fn load_url_bytes(url: url::Url) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .user_agent(concat!(
            "goose/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/aaif-goose/goose)"
        ))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| format!("failed to create HTTP client: {error}"))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("failed to download image: {error}"))?
        .error_for_status()
        .map_err(|error| format!("failed to download image: {error}"))?;

    collect_response_bytes(response, MAX_IMAGE_BYTES).await
}

async fn collect_response_bytes(
    mut response: reqwest::Response,
    max_bytes: u64,
) -> Result<Vec<u8>, String> {
    if let Some(len) = response.content_length() {
        ensure_size_limit(len, max_bytes)?;
    }

    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("failed to read image response: {error}"))?
    {
        let next_len = bytes
            .len()
            .checked_add(chunk.len())
            .ok_or_else(|| size_limit_error(u64::MAX, max_bytes))?;
        ensure_size_limit(next_len as u64, max_bytes)?;
        bytes.extend_from_slice(&chunk);
    }

    Ok(bytes)
}

fn load_file_bytes(path: PathBuf) -> Result<Vec<u8>, String> {
    #[cfg(windows)]
    {
        with_windows_local_path(&path, read_file_bytes)
    }
    #[cfg(not(windows))]
    {
        read_file_bytes(&path)
    }
}

#[cfg(any(windows, test))]
fn with_windows_local_path<T>(
    path: &Path,
    action: impl FnOnce(&Path) -> Result<T, String>,
) -> Result<T, String> {
    validate_windows_image_path(path)?;
    action(path)
}

#[cfg(any(windows, test))]
fn validate_windows_image_path(path: &Path) -> Result<(), String> {
    let bytes = path.as_os_str().as_encoded_bytes();
    let local_verbatim_drive = bytes.starts_with(br"\\?\")
        && matches!(bytes.get(4..7), Some([drive, b':', b'\\']) if drive.is_ascii_alphabetic());
    let double_separator = matches!(bytes, [b'\\' | b'/', b'\\' | b'/', ..]);
    let nt_namespace = matches!(bytes, [b'\\' | b'/', b'?', b'?', b'\\' | b'/', ..]);
    if (double_separator && !local_verbatim_drive) || nt_namespace {
        return Err("network and device paths are not supported for images".to_string());
    }
    Ok(())
}

fn read_file_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let file =
        std::fs::File::open(path).map_err(|error| format!("failed to read image file: {error}"))?;
    let file_size = file
        .metadata()
        .map_err(|error| format!("failed to read image file: {error}"))?
        .len();
    ensure_image_size(file_size)?;

    let bytes = read_bounded(file, MAX_IMAGE_BYTES)
        .map_err(|error| format!("failed to read image file: {error}"))?;
    ensure_image_size(bytes.len() as u64)?;
    Ok(bytes)
}

fn read_bounded(reader: impl Read, max_bytes: u64) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(max_bytes + 1).read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn validate_crop(crop: &CropParams, image_width: u32, image_height: u32) -> Result<(), String> {
    if crop.width == 0 || crop.height == 0 {
        return Err("crop width and height must be greater than zero".to_string());
    }

    let right = crop
        .x
        .checked_add(crop.width)
        .ok_or_else(|| "crop rectangle is out of bounds".to_string())?;
    let bottom = crop
        .y
        .checked_add(crop.height)
        .ok_or_else(|| "crop rectangle is out of bounds".to_string())?;

    if right > image_width || bottom > image_height {
        return Err(format!(
            "crop rectangle {}x{} at {},{} exceeds image bounds {}x{}",
            crop.width, crop.height, crop.x, crop.y, image_width, image_height
        ));
    }

    Ok(())
}

fn ensure_image_size(len: u64) -> Result<(), String> {
    ensure_size_limit(len, MAX_IMAGE_BYTES)
}

fn ensure_size_limit(len: u64, max_bytes: u64) -> Result<(), String> {
    if len > max_bytes {
        Err(size_limit_error(len, max_bytes))
    } else {
        Ok(())
    }
}

fn size_limit_error(len: u64, max_bytes: u64) -> String {
    format!("image is too large: {len} bytes exceeds {max_bytes} byte limit")
}

fn mime_type(format: image::ImageFormat) -> Result<&'static str, String> {
    match format {
        image::ImageFormat::Png => Ok("image/png"),
        image::ImageFormat::Jpeg => Ok("image/jpeg"),
        image::ImageFormat::Gif => Ok("image/gif"),
        image::ImageFormat::WebP => Ok("image/webp"),
        _ => Err(
            "unsupported image format; supported formats are png, jpeg, gif, and webp".to_string(),
        ),
    }
}
#[cfg(test)]
mod local_file_tests {
    use super::*;

    const SMALL_PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";

    #[test]
    fn windows_network_paths_are_rejected_before_file_access() {
        for path in [
            r"\\server.invalid\share\image.png",
            "//server.invalid/share/image.png",
            r"\/server.invalid\share\image.png",
            r"/\server.invalid/share/image.png",
            r"\\?\UNC\server.invalid\share\image.png",
            r"\\?\unc\server.invalid\share\image.png",
            r"\\.\UNC\server.invalid\share\image.png",
            r"\\.\pipe\image",
            r"\\?\GLOBALROOT\Device\Mup\server.invalid\share\image.png",
            r"\??\UNC\server.invalid\share\image.png",
            r"/??\UNC\server.invalid\share\image.png",
            r"\??/UNC/server.invalid/share/image.png",
            r"\\?\C:relative.png",
            r"\\?",
            r"\\?\",
            r"\\?\C",
            r"\\?\C:",
            r"\\?\C:/image.png",
            r"//?/C:/image.png",
            r"\\?\1:\image.png",
            "//",
            r"\\",
        ] {
            let mut accessed = false;
            let result = with_windows_local_path(Path::new(path), |_| {
                accessed = true;
                Ok(())
            });
            assert!(!accessed, "filesystem action reached for {path:?}");
            assert_eq!(
                result.unwrap_err(),
                "network and device paths are not supported for images"
            );
        }
    }

    #[test]
    fn windows_local_paths_reach_file_access_unchanged() {
        for path in [
            r"C:\images\picture.png",
            "C:/images/picture.png",
            r"C:picture.png",
            r"\images\picture.png",
            "picture.png",
            r"images\picture.png",
            r"\\?\C:\images\picture.png",
            r"\\?\c:\images\画像.png",
            r"\\?\C:\",
            "",
            r"\",
            "/",
        ] {
            let observed =
                with_windows_local_path(Path::new(path), |resolved| Ok(resolved.to_path_buf()))
                    .unwrap();
            assert_eq!(observed, Path::new(path));
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_resolved_and_file_url_network_paths_are_rejected_without_io() {
        let mut paths = Vec::new();
        for cwd in [r"\\server.invalid\share", r"\\?\UNC\server.invalid\share"] {
            for source in ["image.png", r"\image.png", "/image.png"] {
                paths.push(resolve_path(source, Some(Path::new(cwd))));
            }
        }
        paths.push(
            url::Url::parse("file://server.invalid/share/image.png")
                .unwrap()
                .to_file_path()
                .unwrap(),
        );
        for source in [
            "file:////server.invalid/share/image.png",
            "file:///%5C%5Cserver.invalid%5Cshare%5Cimage.png",
        ] {
            if let Ok(path) = url::Url::parse(source).unwrap().to_file_path() {
                paths.push(path);
            }
        }

        for source in [
            r"\??\UNC\server.invalid\share\image.png",
            r"\/server.invalid\share",
        ] {
            assert!(validate_windows_image_path(Path::new(source)).is_err());
        }
        for source in [
            "file:///C:/images/picture.png",
            "file://localhost/C:/images/picture.png",
        ] {
            let path = url::Url::parse(source).unwrap().to_file_path().unwrap();
            assert!(with_windows_local_path(&path, |_| Ok(())).is_ok());
        }
        for path in paths {
            let result: Result<(), String> = with_windows_local_path(&path, |_| {
                panic!("filesystem action reached for {path:?}")
            });
            assert!(result.is_err());
        }
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn posix_backslash_filename_is_still_a_local_image() {
        let temp = tempfile::tempdir().unwrap();
        let name = r"\\server.invalid\share\image.png";
        let relative_path = Path::new(name);
        assert!(relative_path.is_relative());
        let png = base64::prelude::BASE64_STANDARD.decode(SMALL_PNG).unwrap();
        std::fs::write(temp.path().join(relative_path), &png).unwrap();
        let bytes = load_image_bytes(name, Some(temp.path())).await.unwrap();
        assert_eq!(bytes, png);
    }

    #[test]
    fn bounded_reader_accepts_limit_and_detects_extra_byte() {
        assert_eq!(read_bounded(&b"12345678"[..], 8).unwrap(), b"12345678");
        assert_eq!(read_bounded(&b"123456789"[..], 8).unwrap(), b"123456789");
    }

    #[test]
    fn bounded_reader_stops_productive_infinite_source() {
        assert_eq!(read_bounded(std::io::repeat(0), 8).unwrap().len(), 9);
    }

    #[test]
    fn oversized_sparse_local_file_is_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("oversized.png");
        std::fs::File::create(&path)
            .unwrap()
            .set_len(MAX_IMAGE_BYTES + 1)
            .unwrap();

        let error = load_file_bytes(path).unwrap_err();

        assert_eq!(
            error,
            format!(
                "image is too large: {} bytes exceeds {MAX_IMAGE_BYTES} byte limit",
                MAX_IMAGE_BYTES + 1
            )
        );
    }

    #[tokio::test]
    async fn local_path_and_file_url_still_decode_small_image() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("small.png");
        let png = base64::prelude::BASE64_STANDARD.decode(SMALL_PNG).unwrap();
        std::fs::write(&path, &png).unwrap();
        let file_url = url::Url::from_file_path(&path).unwrap().to_string();

        for source in [path.to_string_lossy().into_owned(), file_url] {
            let loaded = load_image(&ImageReadParams { source, crop: None }, None)
                .await
                .unwrap();

            assert_eq!(loaded.mime_type, "image/png");
            assert_eq!(loaded.bytes_len, png.len());
            assert_eq!((loaded.width, loaded.height), (1, 1));
            assert_eq!(
                base64::prelude::BASE64_STANDARD
                    .decode(loaded.data)
                    .unwrap(),
                png
            );
        }
    }

    #[tokio::test]
    async fn relative_local_path_still_supports_cropping() {
        let temp = tempfile::tempdir().unwrap();
        let png = base64::prelude::BASE64_STANDARD.decode(SMALL_PNG).unwrap();
        std::fs::write(temp.path().join("small.png"), png).unwrap();
        let loaded = load_image(
            &ImageReadParams {
                source: "small.png".to_string(),
                crop: Some(CropParams {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                }),
            },
            Some(temp.path()),
        )
        .await
        .unwrap();
        assert!(loaded.cropped);
        assert_eq!((loaded.width, loaded.height), (1, 1));
        assert_eq!(loaded.mime_type, "image/png");
    }

    #[tokio::test]
    async fn invalid_local_file_keeps_unsupported_format_error() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("not-an-image.bin");
        std::fs::write(&path, b"not an image").unwrap();

        let error = load_image(
            &ImageReadParams {
                source: path.to_string_lossy().into_owned(),
                crop: None,
            },
            None,
        )
        .await
        .unwrap_err();

        assert_eq!(
            error,
            "unsupported image format; supported formats are png, jpeg, gif, and webp"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    const TEST_MAX_BYTES: u64 = 8;

    async fn serve_response(
        headers: &str,
        body: Vec<u8>,
    ) -> (url::Url, tokio::task::JoinHandle<()>) {
        serve_response_with_version("HTTP/1.1", headers, body).await
    }

    async fn serve_close_delimited_response(
        body: Vec<u8>,
    ) -> (url::Url, tokio::task::JoinHandle<()>) {
        serve_response_with_version("HTTP/1.0", "Connection: close\r\n", body).await
    }

    async fn serve_response_with_version(
        version: &str,
        headers: &str,
        body: Vec<u8>,
    ) -> (url::Url, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mut response = format!("{version} 200 OK\r\n{headers}\r\n").into_bytes();
        response.extend_from_slice(&body);

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let mut buffer = [0; 1024];
                let bytes_read = stream.read(&mut buffer).await.unwrap();
                if bytes_read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..bytes_read]);
            }
            stream.write_all(&response).await.unwrap();
            stream.shutdown().await.unwrap();
        });

        (
            url::Url::parse(&format!("http://{address}/image.png")).unwrap(),
            server,
        )
    }

    async fn fetch_response(url: url::Url) -> reqwest::Response {
        reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .get(url)
            .send()
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn response_at_limit_without_content_length_is_accepted() {
        let (url, server) = serve_close_delimited_response(b"12345678".to_vec()).await;
        let response = fetch_response(url).await;

        let bytes = collect_response_bytes(response, TEST_MAX_BYTES)
            .await
            .unwrap();
        server.await.unwrap();

        assert_eq!(bytes, b"12345678");
    }

    #[tokio::test]
    async fn response_over_limit_without_content_length_is_rejected() {
        let (url, server) = serve_close_delimited_response(b"123456789".to_vec()).await;
        let response = fetch_response(url).await;

        let error = collect_response_bytes(response, TEST_MAX_BYTES)
            .await
            .unwrap_err();
        server.await.unwrap();

        assert_eq!(error, "image is too large: 9 bytes exceeds 8 byte limit");
    }

    #[tokio::test]
    async fn chunked_response_over_limit_is_rejected() {
        let body = b"4\r\n1234\r\n5\r\n56789\r\n0\r\n\r\n".to_vec();
        let (url, server) =
            serve_response("Transfer-Encoding: chunked\r\nConnection: close\r\n", body).await;
        let response = fetch_response(url).await;

        let error = collect_response_bytes(response, TEST_MAX_BYTES)
            .await
            .unwrap_err();
        server.await.unwrap();

        assert_eq!(error, "image is too large: 9 bytes exceeds 8 byte limit");
    }

    #[tokio::test]
    async fn oversized_content_length_is_rejected_before_collection() {
        let (url, server) =
            serve_response("Content-Length: 9\r\nConnection: close\r\n", Vec::new()).await;
        let response = fetch_response(url).await;

        let error = collect_response_bytes(response, TEST_MAX_BYTES)
            .await
            .unwrap_err();
        server.await.unwrap();

        assert_eq!(error, "image is too large: 9 bytes exceeds 8 byte limit");
    }

    #[tokio::test]
    async fn small_png_response_still_decodes() {
        let png = base64::prelude::BASE64_STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
            .unwrap();
        let (url, server) = serve_response(
            &format!(
                "Content-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n",
                png.len()
            ),
            png.clone(),
        )
        .await;
        let params = ImageReadParams {
            source: url.to_string(),
            crop: None,
        };

        let loaded = load_image(&params, None).await.unwrap();
        server.await.unwrap();

        assert_eq!(loaded.mime_type, "image/png");
        assert_eq!(loaded.bytes_len, png.len());
        assert_eq!((loaded.width, loaded.height), (1, 1));
    }
}
