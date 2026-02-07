use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::{fs, time::Duration};

use reqwest::StatusCode;

// Integration test for reload using a temporary file location.
// Copies `examples/counter.vcd` to a temp dir as `counter.vcd`, starts the server
// on that path, then overwrites it with `examples/counter2.vcd` and triggers reload.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn server_reload_with_overwrite() {
    // Choose a free port
    let port = {
        let sock = std::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .expect("bind 0");
        sock.local_addr().unwrap().port()
    };

    // Source example files
    let mut examples = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    examples.push("..");
    examples.push("examples");
    let src1 = examples.join("counter.vcd");
    let src2 = examples.join("counter2.vcd");

    // Prepare a temporary directory and destination file path with automatic cleanup
    let tmpdir = tempfile::tempdir().expect("create temp dir");
    let dest = tmpdir.path().join("counter.vcd");

    // Copy initial file to destination
    fs::copy(&src1, &dest).expect("copy counter.vcd to temp");

    let token = "reloadtoken123456".to_string(); // >= MIN_TOKEN_LEN

    let started = Arc::new(AtomicBool::new(false));
    let started_clone = started.clone();
    let token_clone = token.clone();
    let dest_clone = dest.clone();

    // Start server on the temp path
    let handle = tokio::spawn(async move {
        if let Err(err) = surver::surver_main(
            port,
            "127.0.0.1".to_string(),
            Some(token_clone),
            &[dest_clone.to_string_lossy().to_string()],
            Some(started_clone),
        )
        .await
        {
            eprintln!("server_main error: {err:?}");
        }
    });

    // Wait until started
    let mut waited_ms = 0;
    while !started.load(Ordering::SeqCst) && waited_ms < 5000 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        waited_ms += 50;
    }
    assert!(
        started.load(Ordering::SeqCst),
        "server did not start in time"
    );

    let base = format!("http://127.0.0.1:{port}/{token}");
    let client = reqwest::Client::new();

    // Ensure initial body load completes
    let mut initial_loaded = false;
    for _ in 0..100 {
        // ~10s
        let st_resp = client
            .get(format!("{base}/get_status"))
            .send()
            .await
            .unwrap();
        assert_eq!(st_resp.status(), StatusCode::OK);
        let st_body = st_resp.bytes().await.unwrap();
        let st: surver::SurverStatus = serde_json::from_slice(&st_body).unwrap();
        if st.file_infos[0].last_load_ok {
            initial_loaded = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(initial_loaded, "initial load did not complete in time");

    // Overwrite the destination file with a different VCD
    fs::copy(&src2, &dest).expect("overwrite counter.vcd with counter2.vcd");

    // Update the file modification time to ensure it's detected as changed
    #[cfg(target_os = "windows")]
    {
        use std::time::SystemTime;
        let now = filetime::FileTime::from_system_time(SystemTime::now());
        filetime::set_file_mtime(&dest, now).expect("set mtime");
    }

    // Wait a moment to ensure filesystem sees the change
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Trigger reload; should be accepted (202)
    let resp = client.get(format!("{base}/0/reload")).send().await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::ACCEPTED,
        "reload after overwrite should be accepted"
    );

    // Poll status until reload completes
    let mut reload_complete = false;
    for _ in 0..100 {
        // up to ~10s
        let st_resp = client
            .get(format!("{base}/get_status"))
            .send()
            .await
            .unwrap();
        assert_eq!(st_resp.status(), StatusCode::OK);
        let st_body = st_resp.bytes().await.unwrap();
        let st: surver::SurverStatus = serde_json::from_slice(&st_body).unwrap();
        if !st.file_infos[0].reloading && st.file_infos[0].last_load_ok {
            reload_complete = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(reload_complete, "reload did not finish in expected time");

    // 2) Reload with unchanged file -> 304 Not Modified
    let resp = client.get(format!("{base}/0/reload")).send().await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::NOT_MODIFIED,
        "unchanged file should yield 304"
    );

    // Cleanup: stop server (temp dir is automatically cleaned up when tmpdir is dropped)
    handle.abort();
}
