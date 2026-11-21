//! `herdr-app do <command> [--arg value ... | json]`, `herdr-app ls`, `herdr-app commands`
//! and `herdr-app status`: drive a running window from a shell (agents, tests)
//! over `$HERDR_APP_HOME/herdr-app.sock`, one JSON line each way.

use crate::mods::{self, Request};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
#[cfg(windows)]
use uds_windows::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::Duration;

#[cfg(target_os = "macos")]
struct HomeClaim {
    listener: Option<UnixListener>,
    _lock: std::fs::File,
}
#[cfg(target_os = "macos")]
static HOME_CLAIM: std::sync::Mutex<Option<HomeClaim>> = std::sync::Mutex::new(None);

/// Reserve the shared home before reading settings or opening a window.
/// The advisory lock serializes fork-vs-fork launches; the socket also detects
/// a normal official Herdr instance which does not participate in our lock.
#[cfg(target_os = "macos")]
fn claim_home_at(path: &std::path::Path) -> Result<HomeClaim, String> {
    use std::os::unix::fs::{FileTypeExt, OpenOptionsExt};
    let occupied = || "Herdr is already running. Quit it before opening Herdr WebView.".to_owned();
    if UnixStream::connect(path).is_ok() { return Err(occupied()); }
    let parent = path.parent().ok_or("invalid Herdr config home")?;
    std::fs::create_dir_all(parent).map_err(|e| format!("cannot prepare shared home: {e}"))?;
    let lock_path = path.with_extension("webview.lock");
    let lock = std::fs::OpenOptions::new().read(true).write(true).create(true)
        .mode(0o600).custom_flags(libc::O_NOFOLLOW).open(&lock_path)
        .map_err(|e| format!("cannot open WebView owner lock: {e}"))?;
    lock.try_lock().map_err(|_| occupied())?;
    // Recheck while holding the lock to narrow races with the official app.
    if UnixStream::connect(path).is_ok() { return Err(occupied()); }
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if !meta.file_type().is_socket() {
            return Err("refusing to replace a non-socket at Herdr's control path".into());
        }
        std::fs::remove_file(path).map_err(|e| format!("cannot clear stale control socket: {e}"))?;
    }
    let listener = UnixListener::bind(path).map_err(|e| format!("cannot claim shared Herdr socket: {e}"))?;
    Ok(HomeClaim { listener: Some(listener), _lock: lock })
}

#[cfg(target_os = "macos")]
pub fn claim_home() -> Result<(), String> {
    let claim = claim_home_at(&socket())?;
    *HOME_CLAIM.lock().unwrap() = Some(claim);
    Ok(())
}

fn socket() -> PathBuf {
    mods::app_dir().join("herdr-app.sock")
}

/// Serves the socket for this window. WebView release builds must use
/// the already-owned listener; they cannot silently proceed with no socket.
pub fn listen(tx: async_channel::Sender<Request>) {
    #[cfg(target_os = "macos")]
    let prebound = if option_env!("HERDR_BROWSER_FORK").is_some() {
        Some(HOME_CLAIM.lock().unwrap().as_mut()
            .and_then(|claim| claim.listener.take())
            .expect("WebView startup must own its shared-home socket"))
    } else { None };
    #[cfg(not(target_os = "macos"))]
    let prebound: Option<UnixListener> = None;
    let listener = if let Some(listener) = prebound {
        listener
    } else {
        let path = socket();
        if UnixStream::connect(&path).is_ok() {
            tracing::warn!("another Herdr is serving {}; `herdr-app do` goes there", path.display());
            return;
        }
        let _ = std::fs::remove_file(&path);
        match UnixListener::bind(&path) {
            Ok(l) => l,
            Err(e) => return tracing::warn!("cannot serve {}: {e}", path.display()),
        }
    };
    let _ = std::thread::Builder::new().name("herdr-app-control".into()).spawn(move || {
        for stream in listener.incoming().flatten() {
            let tx = tx.clone();
            std::thread::spawn(move || serve(stream, tx));
        }
    });
}

fn serve(stream: UnixStream, tx: async_channel::Sender<Request>) {
    let mut line = String::new();
    if BufReader::new(&stream).read_line(&mut line).is_err() {
        return;
    }
    let reply = match serde_json::from_str::<Value>(&line) {
        Ok(cmd) => {
            let timeout = Duration::from_secs(if cmd.get("shot").is_some() { 30 } else { 5 });
            let (reply_tx, reply_rx) = std::sync::mpsc::channel();
            let _ = tx.send_blocking(Request::Control { cmd, reply: reply_tx });
            reply_rx.recv_timeout(timeout).unwrap_or_else(|_| json!({ "error": "the window did not answer" }))
        }
        Err(e) => json!({ "error": format!("bad request: {e}") }),
    };
    let _ = (&stream).write_all(format!("{reply}\n").as_bytes());
}

/// One Herdr per home: a second launch brings the running window to the
/// front and quits. True when one is running (even one too busy to answer:
/// a second window would ring every alert twice).
pub fn raise_running() -> bool {
    raise_running_at(&socket())
}

fn raise_running_at(path: &std::path::Path) -> bool {
    let Ok(mut stream) = UnixStream::connect(path) else { return false };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    if stream.write_all(format!("{}\n", json!({ "raise": true })).as_bytes()).is_ok() {
        let _ = BufReader::new(&stream).read_line(&mut String::new());
    }
    true
}

/// The client side; returns the process exit code.
pub fn client(args: &[String]) -> i32 {
    if args == ["commands"] {
        println!("{}", serde_json::to_string_pretty(&crate::commands::listing()).unwrap());
        return 0;
    }
    let cmd = match args {
        [verb, rest @ ..] if verb == "shot" && rest.len() <= 1 => {
            let path = PathBuf::from(rest.first().map(String::as_str).unwrap_or("herdr-shot.png"));
            let path = match std::path::absolute(path) {
                Ok(path) => path,
                Err(e) => return fail(&format!("cannot resolve screenshot path: {e}")),
            };
            if path.extension().is_none_or(|ext| !ext.eq_ignore_ascii_case("png")) {
                return fail("screenshot path must end in .png");
            }
            json!({ "shot": path })
        },
        [verb] if verb == "status" => json!({ "status": true }),
        [verb] if verb == "ls" => json!({ "ls": true }),
        [verb, message] if verb == "feedback" => json!({ "do": "app.feedback", "args": { "message": message, "from": "agent" } }),
        [verb, command, rest @ ..] if verb == "do" => match parse_args(rest) {
            Ok(a) => json!({ "do": command, "args": a }),
            Err(e) => return fail(&e),
        },
        _ => return fail("usage: herdr-app ls | herdr-app commands | herdr-app status | herdr-app shot [file.png] | herdr-app do <command> [--arg value ... | json]"),
    };
    let mut stream = match UnixStream::connect(socket()) {
        Ok(s) => s,
        Err(_) if args.first().is_some_and(|v| v == "feedback") => {
            let body = crate::feedback::draft(&args[1], "", 0, "agent", "other");
            println!("{}", serde_json::to_string_pretty(&body).unwrap_or_default());
            eprintln!("Herdr must be open to review and send this feedback. Nothing was sent; core version and layout will reflect the open window.");
            return 1;
        }
        Err(e) => return fail(&format!("no Herdr window at {}: {e}", socket().display())),
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(35)));
    let mut line = String::new();
    if stream.write_all(format!("{cmd}\n").as_bytes()).is_err() || BufReader::new(&stream).read_line(&mut line).is_err() {
        return fail("the window hung up");
    }
    let reply: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
    if args.first().is_some_and(|v| v == "shot") {
        if let Some(error) = reply.get("error") {
            return fail(error.as_str().unwrap_or("screenshot failed"));
        }
        let Some(path) = reply.get("path").and_then(Value::as_str) else {
            return fail("the window returned no screenshot path");
        };
        println!("{path}");
    } else {
        println!("{}", serde_json::to_string_pretty(&reply).unwrap_or_default());
    }
    if reply.get("error").is_some() { 1 } else { 0 }
}

/// `--name api --argv '["pi"]'` (values that parse as JSON are JSON), or one JSON object.
fn parse_args(rest: &[String]) -> Result<Value, String> {
    match rest {
        [] => Ok(Value::Null),
        [raw] if raw.trim_start().starts_with('{') => serde_json::from_str(raw).map_err(|e| format!("args are not JSON: {e}")),
        _ => {
            let mut out = serde_json::Map::new();
            let mut it = rest.iter();
            while let Some(flag) = it.next() {
                let key = flag.strip_prefix("--").ok_or_else(|| format!("expected --<arg>, got {flag:?}"))?;
                let value = it.next().ok_or_else(|| format!("--{key} needs a value"))?;
                let v = serde_json::from_str::<Value>(value).ok().filter(|v| !v.is_string()).unwrap_or_else(|| Value::String(value.clone()));
                out.insert(key.to_owned(), v);
            }
            Ok(Value::Object(out))
        }
    }
}

/// Encode before opening the destination: GPU errors must not truncate a good shot.
pub fn save_shot<C: gpui::AppContext>(
    window: gpui::AnyWindowHandle,
    cx: &mut C,
    path: &std::path::Path,
) -> anyhow::Result<()> {
    let image = window.render_current_to_image(cx)?;
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, image.width(), image.height());
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
        encoder.write_header()?.write_image_data(image.as_raw())?;
    }
    // A sibling temporary file keeps publication on the same filesystem.
    let temporary = path.with_file_name(format!(".herdr-shot-{}.png", std::process::id()));
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temporary)?;
    let result = (|| -> anyhow::Result<()> {
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn fail(message: &str) -> i32 {
    eprintln!("herdr-app: {message}");
    2
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn flag_values_that_parse_as_json_are_json() {
        let args: Vec<String> = ["--name", "eng-123", "--focus", "false", "--argv", r#"["claude"]"#].map(String::from).into();
        assert_eq!(parse_args(&args).unwrap(), json!({ "name": "eng-123", "focus": false, "argv": ["claude"] }));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn fork_claim_refuses_second_owner_and_recovers_stale_socket() {
        let dir = std::env::temp_dir().join(format!("herdr-fork-owner-{}-{:?}",
            std::process::id(), std::thread::current().id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("herdr-app.sock");
        let first = claim_home_at(&path).unwrap();
        assert!(claim_home_at(&path).is_err(), "second owner must be rejected");
        drop(first);
        let recovered = claim_home_at(&path).unwrap();
        drop(recovered);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn fork_claim_refuses_to_unlink_unrelated_files() {
        let dir = std::env::temp_dir().join(format!("herdr-fork-non-socket-{}",std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("herdr-app.sock");
        std::fs::write(&path,"keep me").unwrap();
        assert!(claim_home_at(&path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(),"keep me");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_second_launch_raises_the_running_window() {
        let dir = std::env::temp_dir().join(format!("herdr-app-raise-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("herdr-app.sock");
        // Nothing running: this launch opens its own window.
        assert!(!raise_running_at(&path));
        // One running: it gets asked to come to the front.
        let listener = UnixListener::bind(&path).unwrap();
        let asked = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            (&stream).write_all(b"{\"ok\":true}\n").unwrap();
            serde_json::from_str::<Value>(&line).unwrap()
        });
        assert!(raise_running_at(&path));
        assert_eq!(asked.join().unwrap(), json!({ "raise": true }));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
