//! Handling of external communication in Surver.
use bincode::Options;
use eyre::{Context, Result, anyhow, bail};
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::collections::HashMap;
use std::fs;
use std::iter::repeat_with;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, RwLock};
use std::time::{Instant, SystemTime};
use tokio::net::TcpListener;
use tokio::sync::Notify;
use tracing::{error, info, warn};
use wellen::{
    CompressedSignal, CompressedTimeTable, FileFormat, Hierarchy, Signal, SignalRef, Time, viewers,
};

use crate::{
    BINCODE_OPTIONS, HTTP_SERVER_KEY, HTTP_SERVER_VALUE_SURFER, SURFER_VERSION, SurverFileInfo,
    SurverStatus, WELLEN_SURFER_DEFAULT_OPTIONS, WELLEN_VERSION, X_SURFER_VERSION,
    X_WELLEN_VERSION, modification_time_string,
};

struct ReadOnly {
    url: String,
    token: String,
}

struct FileInfo {
    filename: String,
    hierarchy: Hierarchy,
    file_format: FileFormat,
    header_len: u64,
    body_len: u64,
    body_progress: Arc<AtomicU64>,
    notify: Arc<Notify>,
    timetable: Vec<Time>,
    signals: HashMap<SignalRef, Signal>,
    reloading: bool,
    last_reload_ok: bool,
    last_reload_time: Option<Instant>,
    last_modification_time: Option<SystemTime>,
}

#[derive(Default)]
struct SurverState {
    file_infos: Vec<FileInfo>,
}

impl FileInfo {
    fn modification_time_string(&self) -> String {
        modification_time_string(self.last_modification_time)
    }

    fn reload_time_string(&self) -> String {
        if let Some(time) = self.last_reload_time {
            return format!("{:?} ago", time.elapsed());
        }
        "never".to_string()
    }

    pub fn html_table_line(&self) -> String {
        let bytes_loaded = self.body_progress.load(Ordering::SeqCst);

        let progress = if bytes_loaded == self.body_len {
            format!(
                "{} loaded",
                bytesize::ByteSize::b(self.body_len + self.header_len)
            )
        } else {
            format!(
                "{} / {}",
                bytesize::ByteSize::b(bytes_loaded + self.header_len),
                bytesize::ByteSize::b(self.body_len + self.header_len)
            )
        };

        format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            self.filename,
            progress,
            self.modification_time_string(),
            self.reload_time_string()
        )
    }
}

impl From<&FileInfo> for SurverFileInfo {
    fn from(file_info: &FileInfo) -> Self {
        Self {
            bytes: file_info.body_len + file_info.header_len,
            bytes_loaded: file_info.body_progress.load(Ordering::SeqCst) + file_info.header_len,
            filename: file_info.filename.clone(),
            format: file_info.file_format,
            reloading: file_info.reloading,
            last_load_ok: file_info.last_reload_ok,
            last_modification_time: file_info.last_modification_time,
        }
    }
}
enum LoaderMessage {
    SignalRequest(SignalRequest),
    Reload,
}

type SignalRequest = Vec<SignalRef>;

fn get_info_page(shared: &Arc<ReadOnly>, state: &Arc<RwLock<SurverState>>) -> String {
    let state_guard = state.read().expect("State lock poisoned in get_info_page");
    let html_table_content = state_guard
        .file_infos
        .iter()
        .map(FileInfo::html_table_line)
        .collect::<Vec<_>>()
        .join("\n");
    drop(state_guard);

    format!(
        r#"
    <!DOCTYPE html><html lang="en">
    <head>
    <link rel="icon" href="favicon.ico" sizes="any">
    <title>Surver - Surfer Remote Server</title>
    </head>
    <body>
    <h1>Surver - Surfer Remote Server</h1>
    <b>To connect, run:</b> <code>surfer {}</code><br>
    <b>Wellen version:</b> {WELLEN_VERSION}<br>
    <b>Surfer version:</b> {SURFER_VERSION}<br>
    <table border="1" cellpadding="5" cellspacing="0">
    <tr><th>Filename</th><th>Load progress</th><th>File modification time</th><th>(Re)load time</th></tr>
    {}
    </table>
    </body></html>
    "#,
        shared.url, html_table_content
    )
}

fn get_hierarchy(state: &Arc<RwLock<SurverState>>, file_index: usize) -> Result<Vec<u8>> {
    let state_guard = state.read().expect("State lock poisoned in get_hierarchy");
    let file_info = &state_guard.file_infos[file_index];
    let mut raw = BINCODE_OPTIONS.serialize(&file_info.file_format)?;
    let mut raw2 = BINCODE_OPTIONS.serialize(&file_info.hierarchy)?;
    drop(state_guard);
    raw.append(&mut raw2);
    let compressed = lz4_flex::compress_prepend_size(&raw);
    info!(
        "Sending hierarchy. {} raw, {} compressed.",
        bytesize::ByteSize::b(raw.len() as u64),
        bytesize::ByteSize::b(compressed.len() as u64)
    );
    Ok(compressed)
}

async fn get_timetable(state: &Arc<RwLock<SurverState>>, file_index: usize) -> Result<Vec<u8>> {
    // poll to see when the time table is available
    #[allow(unused_assignments)]
    let mut table = vec![];
    loop {
        {
            let state = state.read().unwrap();
            if !state.file_infos[file_index].timetable.is_empty() {
                table.clone_from(&state.file_infos[file_index].timetable);
                break;
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
    let raw_size = table.len() * std::mem::size_of::<Time>();
    let compressed = BINCODE_OPTIONS.serialize(&CompressedTimeTable::compress(&table))?;
    info!(
        "Sending timetable. {} raw, {} compressed.",
        bytesize::ByteSize::b(raw_size as u64),
        bytesize::ByteSize::b(compressed.len() as u64)
    );
    Ok(compressed)
}

fn get_status(state: &Arc<RwLock<SurverState>>) -> Result<Vec<u8>> {
    let state_guard = state.read().expect("State lock poisoned in get_status");
    let file_infos = state_guard
        .file_infos
        .iter()
        .map(SurverFileInfo::from)
        .collect::<Vec<_>>();
    drop(state_guard);
    let status = SurverStatus {
        wellen_version: WELLEN_VERSION.to_string(),
        surfer_version: SURFER_VERSION.to_string(),
        file_infos,
    };
    Ok(serde_json::to_vec(&status)?)
}

async fn get_signals(
    state: &Arc<RwLock<SurverState>>,
    file_index: usize,
    txs: &[Sender<LoaderMessage>],
    id_strings: &[&str],
) -> Result<Vec<u8>> {
    let ids = id_strings
        .iter()
        .map(|id_str| {
            id_str
                .parse::<u64>()
                .map_err(|e| anyhow!("Failed to parse signal id `{id_str}`: {e:#}"))
                .and_then(|index| {
                    SignalRef::from_index(index as usize)
                        .ok_or_else(|| anyhow!("Invalid signal index: {}", index))
                })
        })
        .collect::<Result<Vec<SignalRef>>>()?;

    if ids.is_empty() {
        return Ok(vec![]);
    }
    let num_ids = ids.len();

    // send request to background thread
    txs[file_index].send(LoaderMessage::SignalRequest(ids.clone()))?;

    let notify = {
        let state_guard = state.read().expect("State lock poisoned in get_signals");
        state_guard.file_infos[file_index].notify.clone()
    };

    // Wait for all signals to be loaded
    let mut data = vec![];
    leb128::write::unsigned(&mut data, num_ids as u64)?;
    let mut raw_size = 0;
    loop {
        {
            let state_guard = state.read().expect("State lock poisoned in get_signals");
            if ids
                .iter()
                .all(|id| state_guard.file_infos[file_index].signals.contains_key(id))
            {
                for id in ids {
                    let signal = &state_guard.file_infos[file_index].signals[&id];
                    raw_size += BINCODE_OPTIONS.serialize(signal)?.len();
                    let comp = CompressedSignal::compress(signal);
                    data.append(&mut BINCODE_OPTIONS.serialize(&comp)?);
                }
                break;
            }
        };
        // Wait for notification that signals have been loaded
        notify.notified().await;
    }
    info!(
        "Sending {} signals. {} raw, {} compressed.",
        num_ids,
        bytesize::ByteSize::b(raw_size as u64),
        bytesize::ByteSize::b(data.len() as u64)
    );
    Ok(data)
}

const CONTENT_TYPE: &str = "Content-Type";
const JSON_MIME: &str = "application/json";
const OCTET_MIME: &str = "application/octet-stream";
const HTML_MIME: &str = "text/html; charset=utf-8";

trait DefaultHeader {
    fn default_header(self) -> Self;
}

impl DefaultHeader for hyper::http::response::Builder {
    fn default_header(self) -> Self {
        self.header(HTTP_SERVER_KEY, HTTP_SERVER_VALUE_SURFER)
            .header(X_WELLEN_VERSION, WELLEN_VERSION)
            .header(X_SURFER_VERSION, SURFER_VERSION)
            .header("Cache-Control", "no-cache")
    }
}

fn build_response(
    status: StatusCode,
    content_type: &str,
    body: Vec<u8>,
) -> Result<Response<Full<Bytes>>> {
    Ok(Response::builder()
        .status(status)
        .header(CONTENT_TYPE, content_type)
        .default_header()
        .body(Full::from(body))?)
}

fn not_found_response(message: &[u8]) -> Result<Response<Full<Bytes>>> {
    build_response(StatusCode::NOT_FOUND, OCTET_MIME, message.to_vec())
}

async fn handle_cmd(
    state: &Arc<RwLock<SurverState>>,
    txs: &[Sender<LoaderMessage>],
    cmd: &str,
    file_index: Option<usize>,
    args: &[&str],
) -> Result<Response<Full<Bytes>>> {
    let response = match (file_index, cmd, args) {
        (_, "get_status", []) => {
            let body = get_status(state)?;
            build_response(StatusCode::OK, JSON_MIME, body)
        }
        (Some(file_index), "get_hierarchy", []) => {
            let body = get_hierarchy(state, file_index)?;
            build_response(StatusCode::OK, OCTET_MIME, body)
        }
        (Some(file_index), "get_time_table", []) => {
            let body = get_timetable(state, file_index).await?;
            build_response(StatusCode::OK, OCTET_MIME, body)
        }
        (Some(file_index), "get_signals", id_strings) => {
            let body = get_signals(state, file_index, txs, id_strings).await?;
            build_response(StatusCode::OK, OCTET_MIME, body)
        }
        (Some(file_index), "reload", []) => {
            let mut state_guard = state.write().expect("State lock poisoned in reload");
            let file_info = &mut state_guard.file_infos[file_index];
            // Check file existence, size, and mtime
            let Ok(meta) = fs::metadata(file_info.filename.clone()) else {
                drop(state_guard);
                return not_found_response(b"error: file not found");
            };
            let mtime = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            // Should probably look at file lengths as well for extra safety, but they are not updated correctly at the moment
            let unchanged =
                file_info.last_modification_time == Some(mtime) && file_info.last_reload_ok;
            if unchanged {
                drop(state_guard);
                return build_response(
                    StatusCode::NOT_MODIFIED,
                    JSON_MIME,
                    b"info: file unchanged".to_vec(),
                );
            }
            file_info.last_modification_time = Some(mtime);
            info!(
                "File modification time updated to {}",
                file_info.modification_time_string()
            );
            file_info.reloading = true;
            file_info.last_reload_ok = false;
            drop(state_guard);
            info!("Reload requested");
            txs[file_index].send(LoaderMessage::Reload)?;
            let body = get_status(state)?;
            build_response(StatusCode::ACCEPTED, JSON_MIME, body)
        }
        _ => {
            // unknown command or unexpected number of arguments
            not_found_response(&[])
        }
    };
    Ok(response?)
}

async fn handle(
    state: Arc<RwLock<SurverState>>,
    shared: Arc<ReadOnly>,
    txs: Vec<Sender<LoaderMessage>>,
    req: Request<hyper::body::Incoming>,
) -> Result<Response<Full<Bytes>>> {
    // Check if favicon is requested
    if req.uri().path() == "/favicon.ico" {
        let favicon_data = include_bytes!("../assets/favicon.ico");
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "image/x-icon")
            .header("Cache-Control", "public, max-age=604800")
            .body(Full::from(&favicon_data[..]))?);
    }
    // check to see if the correct token was received
    let path_parts = req.uri().path().split('/').skip(1).collect::<Vec<_>>();

    // check token
    if let Some(provided_token) = path_parts.first() {
        if *provided_token != shared.token {
            warn!(
                "Received request with invalid token: {provided_token} != {}\n{:?}",
                shared.token,
                req.uri()
            );
            return not_found_response(&[]);
        }
    } else {
        // no token
        warn!("Received request with no token: {:?}", req.uri());
        return not_found_response(&[]);
    }

    // Try to parse file index from path_parts[1]
    let (file_index, cmd_idx) = path_parts
        .get(1)
        .and_then(|s| s.parse::<usize>().ok())
        .map_or((None, 1), |idx| (Some(idx), 2));
    // check command
    let response = if let Some(cmd) = path_parts.get(cmd_idx) {
        handle_cmd(&state, &txs, cmd, file_index, &path_parts[cmd_idx + 1..]).await?
    } else {
        // valid token, but no command => return info
        let body = Full::from(get_info_page(&shared, &state));
        Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, HTML_MIME)
            .default_header()
            .body(body)?
    };

    Ok(response)
}

const MIN_TOKEN_LEN: usize = 8;
const RAND_TOKEN_LEN: usize = 24;

pub type ServerStartedFlag = Arc<std::sync::atomic::AtomicBool>;

pub async fn surver_main(
    port: u16,
    bind_address: String,
    token: Option<String>,
    filenames: &[String],
    started: Option<ServerStartedFlag>,
) -> Result<()> {
    // if no token was provided, we generate one
    let token = token.unwrap_or_else(|| {
        // generate a random ASCII token
        repeat_with(fastrand::alphanumeric)
            .take(RAND_TOKEN_LEN)
            .collect()
    });

    if token.len() < MIN_TOKEN_LEN {
        bail!("Token `{token}` is too short. At least {MIN_TOKEN_LEN} characters are required!");
    }

    let state = Arc::new(RwLock::new(SurverState { file_infos: vec![] }));

    let mut txs: Vec<Sender<LoaderMessage>> = Vec::new();
    // load files
    for (file_index, filename) in filenames.iter().enumerate() {
        let start_read_header = web_time::Instant::now();
        let header_result = wellen::viewers::read_header_from_file(
            filename.clone(),
            &WELLEN_SURFER_DEFAULT_OPTIONS,
        )
        .map_err(|e| anyhow!("{e:?}"))
        .with_context(|| format!("Failed to parse wave file: {filename}"))?;
        info!(
            "Loaded header of {filename} in {:?}",
            start_read_header.elapsed()
        );

        let file_info = FileInfo {
            filename: filename.clone(),
            hierarchy: header_result.hierarchy,
            file_format: header_result.file_format,
            header_len: 0, // FIXME: get value from wellen
            body_len: header_result.body_len,
            body_progress: Arc::new(AtomicU64::new(0)),
            notify: Arc::new(Notify::new()),
            timetable: vec![],
            signals: HashMap::new(),
            reloading: false,
            last_reload_ok: true,
            last_reload_time: None,
            last_modification_time: None,
        };
        {
            let mut state_guard = state.write().expect("State lock poisoned when adding file");
            state_guard.file_infos.push(file_info);
        }
        // channel to communicate with loader
        let (tx, rx) = std::sync::mpsc::channel::<LoaderMessage>();
        txs.push(tx.clone());
        // start work thread
        let state_2 = state.clone();
        std::thread::spawn(move || loader(&state_2, header_result.body, file_index, &rx));
    }
    let ip_addr: std::net::IpAddr = bind_address
        .parse()
        .with_context(|| format!("Invalid bind address: {bind_address}"))?;
    let use_localhost = ip_addr.is_loopback();
    if !use_localhost {
        warn!(
            "Server is binding to {bind_address} instead of 127.0.0.1/0:0:0:0:0:0:0:1 (localhost)"
        );
        warn!("This may make the server accessible from external networks");
        warn!("Surver traffic is unencrypted and unauthenticated - use with caution!");
    }

    // immutable read-only data
    let addr = SocketAddr::new(ip_addr, port);
    let url = format!("http://{addr}/{token}");
    let url_copy = url.clone();
    let token_copy = token.clone();
    let shared = Arc::new(ReadOnly { url, token });

    // print out status
    info!("Starting server on {addr}. To use:");
    info!("1. Setup an ssh tunnel: -L {port}:localhost:{port}");
    let hostname = whoami::hostname();
    if let Ok(hostname) = hostname.as_ref()
        && hostname != "localhost"
        && let Ok(username) = whoami::username()
    {
        info!(
            "   The correct command may be: ssh -L {port}:localhost:{port} {username}@{hostname} "
        );
    }

    info!("2. Start Surfer: surfer {url_copy} ");
    if !use_localhost && let Ok(hostname) = hostname {
        let hosturl = format!("http://{hostname}:{port}/{token_copy}");
        info!("or, if the host is directly accessible:");
        info!("1. Start Surfer: surfer {hosturl} ");
    }
    // create listener and serve it
    let listener = TcpListener::bind(&addr).await?;

    // we have started the server
    if let Some(started) = started {
        started.store(true, Ordering::SeqCst);
    }

    // main server loop
    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);

        let state = state.clone();
        let shared = shared.clone();
        let txs = txs.clone();
        tokio::task::spawn(async move {
            let service =
                service_fn(move |req| handle(state.clone(), shared.clone(), txs.clone(), req));
            if let Err(e) = http1::Builder::new().serve_connection(io, service).await {
                error!("server error: {e}");
            }
        });
    }
}

/// Thread that loads the body and signals.
fn loader(
    state: &Arc<RwLock<SurverState>>,
    mut body_cont: viewers::ReadBodyContinuation<std::io::BufReader<std::fs::File>>,
    file_index: usize,
    rx: &std::sync::mpsc::Receiver<LoaderMessage>,
) -> Result<()> {
    loop {
        // load the body of the file
        let start_load_body = web_time::Instant::now();
        let state_guard = state
            .read()
            .expect("State lock poisoned in loader before body load");
        let file_info = &state_guard.file_infos[file_index];
        let filename = file_info.filename.clone();
        let body_result = viewers::read_body(
            body_cont,
            &file_info.hierarchy,
            Some(file_info.body_progress.clone()),
        )
        .map_err(|e| anyhow!("{e:?}"))
        .with_context(|| format!("Failed to parse body of wave file: {filename}"))?;
        drop(state_guard);
        info!(
            "Loaded body of {} in {:?}",
            filename,
            start_load_body.elapsed()
        );

        // update state with body results
        {
            let mut state_guard = state
                .write()
                .expect("State lock poisoned in loader after body load");
            let file_info = &mut state_guard.file_infos[file_index];
            file_info.timetable = body_result.time_table;
            file_info.signals.clear(); // Clear old signals on reload
            if let Ok(meta) = fs::metadata(&file_info.filename) {
                file_info.last_modification_time = Some(meta.modified()?);
                info!(
                    "File modification time of {} set to {}",
                    filename,
                    file_info.modification_time_string()
                );
            }
            file_info.last_reload_time = Some(Instant::now());
            file_info.reloading = false;
            file_info.last_reload_ok = true;
            file_info.notify.notify_waiters();
        }
        // source is private, only owned by us
        let mut source = body_result.source;

        // process requests for signals to be loaded
        loop {
            let msg = rx.recv()?;

            match msg {
                LoaderMessage::SignalRequest(ids) => {
                    // make sure that we do not load signals that have already been loaded
                    let mut filtered_ids = {
                        let state_guard = state
                            .read()
                            .expect("State lock poisoned in loader signal request");
                        ids.iter()
                            .filter(|id| {
                                !state_guard.file_infos[file_index].signals.contains_key(id)
                            })
                            .copied()
                            .collect::<Vec<_>>()
                    };

                    // check if there is anything left to do
                    if filtered_ids.is_empty() {
                        continue;
                    }

                    // load signals without holding the lock
                    filtered_ids.sort();
                    filtered_ids.dedup();
                    let result = {
                        let state_guard = state
                            .read()
                            .expect("State lock poisoned in loader signal request");
                        source.load_signals(
                            &filtered_ids,
                            &state_guard.file_infos[file_index].hierarchy,
                            true,
                        )
                    };

                    // store signals
                    {
                        let mut state_guard = state
                            .write()
                            .expect("State lock poisoned in loader when storing signals");
                        for (id, signal) in result {
                            state_guard.file_infos[file_index]
                                .signals
                                .insert(id, signal);
                        }
                        state_guard.file_infos[file_index].notify.notify_waiters();
                    }
                }
                LoaderMessage::Reload => {
                    let state_guard = state
                        .read()
                        .expect("State lock poisoned in loader before reload");
                    info!(
                        "Reloading waveform file: {}",
                        state_guard.file_infos[file_index].filename
                    );
                    // Reset progress counter
                    state_guard.file_infos[file_index]
                        .body_progress
                        .store(0, Ordering::SeqCst);

                    // Re-read header to get new body continuation
                    let header_result = wellen::viewers::read_header_from_file(
                        state_guard.file_infos[file_index].filename.clone(),
                        &WELLEN_SURFER_DEFAULT_OPTIONS,
                    )
                    .map_err(|e| anyhow!("{e:?}"))
                    .with_context(|| {
                        format!(
                            "Failed to reload wave file: {}",
                            state_guard.file_infos[file_index].filename
                        )
                    })?;

                    body_cont = header_result.body;
                    break; // Break inner loop to reload the body
                }
            }
        }
    }
}
