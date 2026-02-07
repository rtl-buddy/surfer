use std::fmt::Write as _;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::mpsc::Sender;

use bincode::Options;
use eyre::{Context, Result, anyhow};
use eyre::{bail, eyre};
use reqwest::StatusCode;
use thiserror::Error;
use tracing::{info, warn};
use wellen::CompressedTimeTable;

use surver::{
    BINCODE_OPTIONS, HTTP_SERVER_KEY, HTTP_SERVER_VALUE_SURFER, SURFER_VERSION, SurverStatus,
    WELLEN_VERSION, X_SURFER_VERSION, X_WELLEN_VERSION,
};

use super::HierarchyResponse;
use crate::async_util::{perform_async_work, sleep_ms};
use crate::channels::checked_send;
use crate::message::Message;
use crate::wave_source::{LoadOptions, WaveSource};
use crate::wellen::{BodyResult, HeaderResult};

/// Returns a shared reqwest client to reuse HTTP connections and reduce TLS overhead.
fn get_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

#[derive(Debug, Error)]
pub enum ReloadError {
    #[error("File unchanged since last reload")]
    FileUnchanged,
    #[error("Unexpected response code: {0}")]
    UnexpectedStatus(StatusCode),
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("Parse error: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("Response validation error: {0}")]
    Validation(#[from] eyre::Report),
}

fn check_response(server_url: &str, response: &reqwest::Response) -> Result<()> {
    let server = response
        .headers()
        .get(HTTP_SERVER_KEY)
        .ok_or(eyre!("no server header"))?
        .to_str()?;
    if server != HTTP_SERVER_VALUE_SURFER {
        bail!("Unexpected server {server} from {server_url}");
    }
    let surfer_version = response
        .headers()
        .get(X_SURFER_VERSION)
        .ok_or(eyre!("no surfer version header"))?
        .to_str()?;
    if surfer_version != SURFER_VERSION {
        // this mismatch may be OK as long as the wellen version matches
        info!(
            "Surfer version on the server: {surfer_version} does not match client version {SURFER_VERSION}"
        );
    }
    let wellen_version = response
        .headers()
        .get(X_WELLEN_VERSION)
        .ok_or(eyre!("no wellen version header"))?
        .to_str()?;
    if wellen_version != WELLEN_VERSION {
        bail!(
            "Version incompatibility! The server uses wellen {wellen_version}, our client uses wellen {WELLEN_VERSION}"
        );
    }
    Ok(())
}

async fn get_status(server: String) -> Result<SurverStatus> {
    let client = get_client();
    let response = client.get(format!("{server}/get_status")).send().await?;
    check_response(&server, &response)?;
    let body = response.text().await?;
    let status = serde_json::from_str::<SurverStatus>(&body)?;
    Ok(status)
}

async fn reload(
    server: String,
    file_index: usize,
) -> std::result::Result<SurverStatus, ReloadError> {
    let client = get_client();
    let response = client
        .get(format!("{server}/{file_index}/reload"))
        .send()
        .await?;
    check_response(&server, &response)?;
    let status_code = response.status();
    let body = response.text().await?;
    match status_code {
        StatusCode::NOT_MODIFIED => {
            info!("File unchanged, no reload needed");
            Err(ReloadError::FileUnchanged)
        }
        StatusCode::ACCEPTED => {
            info!("File reloaded at server");
            let status = serde_json::from_str::<SurverStatus>(&body)?;
            Ok(status)
        }
        code => {
            warn!("Unexpected response code: {code}");
            Err(ReloadError::UnexpectedStatus(code))
        }
    }
}

async fn get_hierarchy(server: String, file_index: usize) -> Result<HierarchyResponse> {
    let client = get_client();
    let response = client
        .get(format!("{server}/{file_index}/get_hierarchy"))
        .send()
        .await?;
    check_response(&server, &response)?;
    let compressed = response.bytes().await?;
    let raw = lz4_flex::decompress_size_prepended(&compressed)?;
    let mut reader = std::io::Cursor::new(raw);
    // first we read a value, expecting there to be more bytes
    let opts = BINCODE_OPTIONS.allow_trailing_bytes();
    let file_format: wellen::FileFormat = opts.deserialize_from(&mut reader)?;
    // the last value should consume all remaining bytes
    let hierarchy: wellen::Hierarchy = BINCODE_OPTIONS.deserialize_from(&mut reader)?;
    Ok(HierarchyResponse {
        hierarchy,
        file_format,
    })
}

async fn get_time_table(server: String, file_index: usize) -> Result<Vec<wellen::Time>> {
    let client = get_client();
    let response = client
        .get(format!("{server}/{file_index}/get_time_table"))
        .send()
        .await?;
    check_response(&server, &response)?;
    let compressed_data = response.bytes().await?;
    let compressed: CompressedTimeTable = BINCODE_OPTIONS.deserialize(&compressed_data)?;
    let table = compressed.uncompress();
    Ok(table)
}

// Helper to calculate URL length for a signal index
// Much more efficient than string conversion
// Extracted for testing
#[inline]
fn signal_url_len(index: usize) -> usize {
    index.checked_ilog10().unwrap_or(0) as usize + 2 // +1 for '/', +1 as ilog10 rounds down
}

pub async fn get_signals(
    server: String,
    signals: &[wellen::SignalRef],
    max_url_length: u16,
    file_index: usize,
) -> Result<Vec<(wellen::SignalRef, wellen::Signal)>> {
    if signals.is_empty() {
        return Ok(vec![]);
    }

    let max_url_length = max_url_length as usize;
    let base_url = format!("{server}/{file_index}/get_signals");
    let base_len = base_url.len();

    let mut all_results = Vec::with_capacity(signals.len());
    let mut current_batch = Vec::new();
    let mut current_url_len = base_len;

    for signal in signals {
        // Each signal adds: "/" + digits
        let signal_len = signal_url_len(signal.index());

        // Check if adding this signal would exceed the limit
        if current_url_len + signal_len > max_url_length && !current_batch.is_empty() {
            info!(
                "Fetching batch of {} signals due to URL length limit",
                current_batch.len()
            );
            // Fetch current batch
            let batch_results = get_signals_batch(&base_url, &current_batch).await?;
            all_results.extend(batch_results);

            // Start new batch
            current_batch.clear();
            current_url_len = base_len;
        }

        current_batch.push(*signal);
        current_url_len += signal_len;
    }

    // Fetch remaining batch
    if !current_batch.is_empty() {
        let batch_results = get_signals_batch(&base_url, &current_batch).await?;
        all_results.extend(batch_results);
    }

    Ok(all_results)
}

// Helper to format signal URL
// Extracted for testing
#[inline]
fn format_signal_url(base_url: &str, signals: &[wellen::SignalRef]) -> String {
    let mut url = base_url.to_string();
    for signal in signals {
        write!(url, "/{}", signal.index()).unwrap();
    }
    url
}

async fn get_signals_batch(
    base_url: &str,
    signals: &[wellen::SignalRef],
) -> Result<Vec<(wellen::SignalRef, wellen::Signal)>> {
    let client = get_client();
    let url = format_signal_url(base_url, signals);

    let response = client.get(url).send().await?;
    check_response(base_url, &response)?;
    let data = response.bytes().await?;
    let mut reader = std::io::Cursor::new(data);
    let num_ids: u64 = leb128::read::unsigned(&mut reader)?;
    if num_ids > signals.len() as u64 {
        bail!(
            "Too many signals in response: {num_ids}, expected {}",
            signals.len()
        );
    }
    if num_ids == 0 {
        return Ok(vec![]);
    }

    let opts = BINCODE_OPTIONS.allow_trailing_bytes();
    let mut out = Vec::with_capacity(num_ids as usize);
    for _ in 0..(num_ids - 1) {
        let compressed: wellen::CompressedSignal = opts.deserialize_from(&mut reader)?;
        let signal = compressed.uncompress();
        out.push((signal.signal_ref(), signal));
    }
    // for the final signal, we expect to consume all bytes
    let compressed: wellen::CompressedSignal = BINCODE_OPTIONS.deserialize_from(&mut reader)?;
    let signal = compressed.uncompress();
    out.push((signal.signal_ref(), signal));
    Ok(out)
}

pub fn get_hierarchy_from_server(
    sender: Sender<Message>,
    server: String,
    load_options: LoadOptions,
    file_index: usize,
) {
    let start = web_time::Instant::now();
    let source = WaveSource::Url(server.clone());

    perform_async_work(async move {
        let res = get_hierarchy(server.clone(), file_index)
            .await
            .map_err(|e| anyhow!("{e:?}"))
            .with_context(|| format!("Failed to retrieve hierarchy from remote server {server}"));

        let msg = match res {
            Ok(h) => {
                let header =
                    HeaderResult::Remote(Arc::new(h.hierarchy), h.file_format, server, file_index);
                Message::WaveHeaderLoaded(start, source, load_options, header)
            }
            Err(e) => Message::Error(e),
        };
        checked_send(&sender, msg);
    });
}

pub fn get_time_table_from_server(sender: Sender<Message>, server: String, file_index: usize) {
    let start = web_time::Instant::now();
    let source = WaveSource::Url(server.clone());

    perform_async_work(async move {
        let res = get_time_table(server.clone(), file_index)
            .await
            .map_err(|e| anyhow!("{e:?}"))
            .with_context(|| format!("Failed to retrieve time table from remote server {server}"));

        let msg = match res {
            Ok(table) => Message::WaveBodyLoaded(start, source, BodyResult::Remote(table, server)),
            Err(e) => Message::Error(e),
        };
        checked_send(&sender, msg);
    });
}

pub fn get_server_status(sender: Sender<Message>, server: String, delay_ms: u64) {
    let start = web_time::Instant::now();
    perform_async_work(async move {
        sleep_ms(delay_ms).await;
        let res = get_status(server.clone())
            .await
            .map_err(|e| anyhow!("{e:?}"))
            .with_context(|| format!("Failed to retrieve status from remote server {server}"));

        let msg = match res {
            Ok(status) => Message::SetSurverStatus(start, server, status),
            Err(e) => Message::Error(e),
        };
        checked_send(&sender, msg);
    });
}

pub fn server_reload(
    sender: Sender<Message>,
    server: String,
    load_options: LoadOptions,
    file_index: usize,
) {
    let start = web_time::Instant::now();
    perform_async_work(async move {
        let res = reload(server.clone(), file_index).await;
        let mut request_hierarchy = false;

        let msg = match res {
            Ok(status) => {
                request_hierarchy = true;
                Message::SetSurverStatus(start, server.clone(), status)
            }
            Err(crate::remote::ReloadError::FileUnchanged) => Message::StopProgressTracker,
            Err(e) => {
                let err = anyhow!("{e:?}");
                Message::Error(err)
            }
        };
        checked_send(&sender, msg);
        if request_hierarchy {
            get_hierarchy_from_server(sender, server, load_options, file_index);
        }
    });
}

mod tests {
    #[test]
    fn test_signal_url_length_calculation() {
        use crate::remote::client::signal_url_len;
        // Test edge cases for digit calculation
        assert_eq!(signal_url_len(0), 2); // "/0" -> 2 chars
        assert_eq!(signal_url_len(1), 2); // "/1" -> 2 chars
        assert_eq!(signal_url_len(9), 2); // "/9" -> 2 chars
        assert_eq!(signal_url_len(10), 3); // "/10" -> 3 chars
        assert_eq!(signal_url_len(99), 3); // "/99" -> 3 chars
        assert_eq!(signal_url_len(100), 4); // "/100" -> 4 chars
        assert_eq!(signal_url_len(999), 4); // "/999" -> 4 chars
        assert_eq!(signal_url_len(1000), 5); // "/1000" -> 5 chars
        assert_eq!(signal_url_len(65535), 6); // "/65535" -> 6 chars
    }

    #[test]
    fn test_empty_signals_returns_empty() {
        use crate::remote::get_signals;
        // Create a mock async runtime for testing
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let signals: Vec<wellen::SignalRef> = vec![];
            let result = get_signals("http://localhost:8080".to_string(), &signals, 1000, 0).await;

            // Should return Ok with empty vec without making any network calls
            assert!(result.is_ok());
            assert_eq!(result.unwrap().len(), 0);
        });
    }

    #[test]
    fn test_boundary_signal_indices() {
        use crate::remote::client::signal_url_len;
        // Test that we handle boundary cases correctly
        let boundary_indices = vec![0, 1, 9, 10, 99, 100, 999, 1000, 9999, 10000];

        for idx in boundary_indices {
            let sig_ref = wellen::SignalRef::from_index(idx);
            let len = signal_url_len(sig_ref.unwrap().index());

            // Verify the calculated length matches actual string length
            let actual = format!("/{idx}");
            assert_eq!(
                len,
                actual.len(),
                "URL length calculation mismatch for index {}: expected {}, got {}",
                idx,
                actual.len(),
                len
            );
        }
    }

    #[test]
    fn test_url_construction_format() {
        use crate::remote::client::format_signal_url;
        // Verify URL format matches expected pattern
        let base_url = "http://localhost:8080/get_signals";
        let signals: Vec<wellen::SignalRef> = vec![
            wellen::SignalRef::from_index(1),
            wellen::SignalRef::from_index(42),
            wellen::SignalRef::from_index(999),
        ]
        .into_iter()
        .flatten()
        .collect();

        let url = format_signal_url(base_url, &signals);

        assert_eq!(url, "http://localhost:8080/get_signals/1/42/999");
    }
}
