#![cfg(feature = "tokio")]

use seekwel::Error;
use seekwel::connection::Connection;
use seekwel::prelude::*;

#[seekwel::model]
struct Thing {
    id: u64,
    name: String,
}

#[test]
fn seekwel_calls_work_from_tokio_multi_thread_task() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .build()?;

    runtime.block_on(async {
        let handle = tokio::spawn(async {
            match Connection::memory() {
                Ok(()) | Err(Error::AlreadyInitialized) => {}
                Err(error) => return Err(error),
            }

            Thing::create_table()?;
            Connection::get()?.execute("DELETE FROM thing", ())?;
            Thing::builder().name("Pat").create()?;
            Thing::count()
        });

        let count = handle.await.expect("tokio task should not panic")?;
        assert_eq!(count, 1);
        Ok::<(), Error>(())
    })?;

    Ok(())
}
