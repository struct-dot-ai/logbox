use crate::db;
use crate::git;
use chrono::Utc;
use rusqlite::Connection;
use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

const BATCH_SIZE: usize = 100;
const FLUSH_INTERVAL: Duration = Duration::from_millis(500);

struct SharedBuffer {
    lines: Vec<(String, String)>,
    done: bool,
}

fn flush(conn: &Connection, session_id: &str, buffer: &mut Vec<(String, String)>) {
    if buffer.is_empty() {
        return;
    }
    if let Err(e) = db::insert_logs_batch(conn, session_id, buffer) {
        eprintln!("logbox: failed to write logs: {}", e);
    }
    buffer.clear();
}

pub fn run(session_id: &str, quiet: bool) -> io::Result<()> {
    let shared = Arc::new((
        Mutex::new(SharedBuffer {
            lines: Vec::with_capacity(BATCH_SIZE),
            done: false,
        }),
        Condvar::new(),
    ));

    // First Ctrl+C: swallow it, keep reading stdin so we capture shutdown logs.
    // Second Ctrl+C: force exit immediately.
    let sigint_count = Arc::new(AtomicUsize::new(0));
    let sigint_count_clone = Arc::clone(&sigint_count);
    ctrlc::set_handler(move || {
        let count = sigint_count_clone.fetch_add(1, Ordering::SeqCst);
        if count >= 1 {
            std::process::exit(1);
        }
    })
    .expect("Failed to set Ctrl+C handler");

    // Background flush thread
    let flush_shared = Arc::clone(&shared);
    let flush_session_id = session_id.to_string();
    let flush_thread = thread::spawn(move || {
        let flush_conn = db::open_db().expect("Failed to open database in flush thread");
        let (lock, cvar) = &*flush_shared;

        loop {
            let mut buf = lock.lock().unwrap();

            let result = cvar
                .wait_timeout_while(buf, FLUSH_INTERVAL, |b| b.lines.is_empty() && !b.done)
                .unwrap();
            buf = result.0;

            let mut to_flush = Vec::new();
            std::mem::swap(&mut to_flush, &mut buf.lines);
            let is_done = buf.done;
            drop(buf);

            flush(&flush_conn, &flush_session_id, &mut to_flush);

            if is_done {
                break;
            }
        }
    });

    // Main thread: read stdin, buffer lines
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    let reader = stdin.lock();
    let (lock, cvar) = &*shared;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break, // EOF or error — upstream closed
        };

        if !quiet {
            // Ignore broken pipe on stdout (downstream consumer may have closed)
            let _ = writeln!(stdout, "{}", line);
        }

        let timestamp = Utc::now().to_rfc3339();
        let mut buf = lock.lock().unwrap();
        buf.lines.push((timestamp, line));

        if buf.lines.len() >= BATCH_SIZE {
            cvar.notify_one();
        }
    }

    // Signal done and wait for flush thread
    {
        let mut buf = lock.lock().unwrap();
        buf.done = true;
        cvar.notify_one();
    }
    flush_thread.join().expect("Flush thread panicked");

    Ok(())
}

pub fn start(quiet: bool, cwd: Option<String>) -> io::Result<()> {
    let conn = db::open_db().expect("Failed to open database");
    let session_id = uuid::Uuid::new_v4().to_string();
    let repo = git::repo_name();
    let branch = git::current_branch();
    let commit_sha = git::head_sha();
    let work_dir = cwd.or_else(git::repo_root).or_else(|| {
        std::env::current_dir()
            .ok()
            .map(|p| p.to_string_lossy().to_string())
    });

    db::create_session(
        &conn,
        &session_id,
        repo.as_deref(),
        branch.as_deref(),
        commit_sha.as_deref(),
        work_dir.as_deref(),
    )
    .expect("Failed to create session");

    // Cyan bold for "logbox", dim for details
    eprintln!("\x1b[1;36m📦 logbox\x1b[0m recording → session \x1b[2m{}\x1b[0m", &session_id[..8]);

    let result = run(&session_id, quiet);

    eprintln!("\x1b[1;36m📦 logbox\x1b[0m session \x1b[2m{}\x1b[0m ended", &session_id[..8]);

    result
}
