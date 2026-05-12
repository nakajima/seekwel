use std::path::PathBuf;
use std::sync::{Mutex, OnceLock, mpsc};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use seekwel::Error;
use seekwel::connection::Connection;
use seekwel::prelude::*;

#[seekwel::model]
struct Thing {
    id: u64,
    name: String,
}

static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static DB_PATH: OnceLock<PathBuf> = OnceLock::new();

fn setup() -> Result<std::sync::MutexGuard<'static, ()>, Error> {
    let lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let path = DB_PATH.get_or_init(|| {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("seekwel-concurrency-{suffix}.sqlite"))
    });

    match Connection::file(&path.to_string_lossy()) {
        Ok(()) | Err(Error::AlreadyInitialized) => {}
        Err(error) => return Err(error),
    }

    Thing::create_table()?;
    Connection::get()?.execute("DELETE FROM thing", ())?;
    Ok(lock)
}

#[test]
fn transaction_reads_use_the_pinned_writer_connection() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = setup()?;

    Connection::transaction(|| {
        Thing::builder().name("InTx").create()?;
        let names = Thing::all()?
            .into_iter()
            .map(|thing| thing.name)
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["InTx"]);
        Ok(())
    })?;

    Ok(())
}

#[test]
fn reads_continue_during_open_write_transaction() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = setup()?;
    Thing::builder().name("Before").create()?;

    let (started_tx, started_rx) = mpsc::channel();
    let (finish_tx, finish_rx) = mpsc::channel();

    let tx_thread = thread::spawn(move || {
        Connection::transaction(|| {
            Thing::builder().name("Uncommitted").create()?;
            started_tx.send(()).unwrap();
            finish_rx.recv().unwrap();
            Ok(())
        })
        .map_err(|error| error.to_string())
    });

    started_rx.recv_timeout(Duration::from_secs(5))?;

    let (reader_tx, reader_rx) = mpsc::channel();
    let reader_thread = thread::spawn(move || {
        let result = Thing::all()
            .map(|things| {
                things
                    .into_iter()
                    .map(|thing| thing.name)
                    .collect::<Vec<_>>()
            })
            .map_err(|error| error.to_string());
        reader_tx.send(result).unwrap();
    });

    let names = reader_rx
        .recv_timeout(Duration::from_secs(2))?
        .map_err(std::io::Error::other)?;
    assert_eq!(names, vec!["Before"]);

    finish_tx.send(())?;
    tx_thread.join().unwrap().map_err(std::io::Error::other)?;
    reader_thread.join().unwrap();

    assert_eq!(Thing::count()?, 2);
    Ok(())
}

#[test]
fn concurrent_writes_wait_for_open_write_transaction() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = setup()?;

    let (started_tx, started_rx) = mpsc::channel();
    let (finish_tx, finish_rx) = mpsc::channel();

    let tx_thread = thread::spawn(move || {
        Connection::transaction(|| {
            Thing::builder().name("Held").create()?;
            started_tx.send(()).unwrap();
            finish_rx.recv().unwrap();
            Ok(())
        })
        .map_err(|error| error.to_string())
    });

    started_rx.recv_timeout(Duration::from_secs(5))?;

    let (writer_started_tx, writer_started_rx) = mpsc::channel();
    let (writer_done_tx, writer_done_rx) = mpsc::channel();
    let writer_thread = thread::spawn(move || {
        writer_started_tx.send(()).unwrap();
        let result = Thing::builder()
            .name("After")
            .create()
            .map(|_| ())
            .map_err(|error| error.to_string());
        writer_done_tx.send(result).unwrap();
    });

    writer_started_rx.recv_timeout(Duration::from_secs(5))?;
    assert!(matches!(
        writer_done_rx.recv_timeout(Duration::from_millis(200)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));

    finish_tx.send(())?;
    tx_thread.join().unwrap().map_err(std::io::Error::other)?;
    writer_done_rx
        .recv_timeout(Duration::from_secs(5))?
        .map_err(std::io::Error::other)?;
    writer_thread.join().unwrap();

    assert_eq!(Thing::count()?, 2);
    Ok(())
}
