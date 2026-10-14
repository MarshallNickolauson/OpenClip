//! A tiny loopback HTTP server with Range support so the preview <video> can stream the source
//! file. WKWebView's custom-scheme handlers do not stream media reliably; plain HTTP does.
//! Only files registered through [`url_for`] are ever served, and only on 127.0.0.1.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

struct Server {
    port: u16,
    files: Mutex<HashMap<u64, PathBuf>>,
}

static SERVER: OnceLock<Server> = OnceLock::new();

fn server() -> &'static Server {
    SERVER.get_or_init(|| {
        let l = TcpListener::bind("127.0.0.1:0").expect("bind media server");
        let port = l.local_addr().map(|a| a.port()).unwrap_or(0);
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                std::thread::spawn(move || {
                    let _ = handle(s);
                });
            }
        });
        Server { port, files: Mutex::new(HashMap::new()) }
    })
}

pub fn url_for(path: &str) -> String {
    use std::hash::{Hash, Hasher};
    let s = server();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    let id = h.finish();
    s.files.lock().unwrap().insert(id, PathBuf::from(path));
    let ext = std::path::Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("mp4");
    format!("http://127.0.0.1:{}/{id}.{ext}", s.port)
}

fn handle(stream: TcpStream) -> std::io::Result<()> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(20)))?;
    let mut rd = BufReader::new(stream.try_clone()?);
    let mut out = stream;
    let mut line = String::new();
    rd.read_line(&mut line)?;
    let target = line.split_whitespace().nth(1).unwrap_or("/").to_string();
    let head_only = line.starts_with("HEAD");
    let mut range: Option<(u64, Option<u64>)> = None;
    loop {
        let mut h = String::new();
        if rd.read_line(&mut h)? == 0 || h.trim().is_empty() {
            break;
        }
        if let Some(v) = h.to_ascii_lowercase().strip_prefix("range:") {
            if let Some(r) = v.trim().strip_prefix("bytes=") {
                let mut it = r.splitn(2, '-');
                let a = it.next().and_then(|x| x.trim().parse::<u64>().ok());
                let b = it.next().and_then(|x| x.trim().parse::<u64>().ok());
                if let Some(a) = a {
                    range = Some((a, b));
                }
            }
        }
    }
    let id = target.trim_start_matches('/').split('.').next().and_then(|s| s.parse::<u64>().ok());
    let path = id.and_then(|i| server().files.lock().unwrap().get(&i).cloned());
    let Some(path) = path else {
        return out.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    };
    let mut f = std::fs::File::open(&path)?;
    let len = f.metadata()?.len();
    let mime = match path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref() {
        Some("mov") => "video/quicktime",
        _ => "video/mp4",
    };
    let (start, end, status) = match range {
        Some((a, b)) => (a.min(len.saturating_sub(1)), b.unwrap_or(len - 1).min(len - 1), "206 Partial Content"),
        None => (0, len.saturating_sub(1), "200 OK"),
    };
    let n = end + 1 - start;
    let mut hdr = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nAccept-Ranges: bytes\r\nContent-Length: {n}\r\nCache-Control: no-store\r\nConnection: close\r\n"
    );
    if status.starts_with("206") {
        hdr.push_str(&format!("Content-Range: bytes {start}-{end}/{len}\r\n"));
    }
    hdr.push_str("\r\n");
    out.write_all(hdr.as_bytes())?;
    if head_only {
        return Ok(());
    }
    f.seek(SeekFrom::Start(start))?;
    let mut left = n;
    let mut buf = vec![0u8; 512 * 1024];
    while left > 0 {
        let want = left.min(buf.len() as u64) as usize;
        let r = f.read(&mut buf[..want])?;
        if r == 0 {
            break;
        }
        out.write_all(&buf[..r])?;
        left -= r as u64;
    }
    Ok(())
}
