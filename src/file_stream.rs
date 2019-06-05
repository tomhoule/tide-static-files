use crate::utils::buffer_size;
use bytes::Bytes;
use futures::{
    channel::mpsc::{channel, Receiver, Sender},
    executor::ThreadPool,
    io::SeekFrom,
    task::SpawnExt,
    SinkExt,
};
use num_traits::cast::FromPrimitive;
use std::{
    fs::File,
    io::{Read, Seek},
    ops::Range,
    sync::{Arc, Mutex},
};

lazy_static! {
    // XXX to be improved
    static ref FILE_IO_POOL: Arc<Mutex<ThreadPool>> =
        Arc::new(Mutex::new(ThreadPool::new().unwrap()));
}

pub fn new_file_stream(file: File, range: Range<u64>) -> Receiver<std::io::Result<Bytes>> {
    let (sender, receiver) = channel(0);

    FILE_IO_POOL
        .lock()
        .unwrap()
        .spawn(read_worker(file, range, sender.clone()))
        .expect("file io spawn failed");

    receiver
}

async fn read_worker(
    mut file: File,
    mut range: Range<u64>,
    mut sender: Sender<std::io::Result<Bytes>>,
) {
    if let Err(error) = file.seek(SeekFrom::Start(range.start)) {
        sender.send(Err(error)).await.unwrap();
        return;
    }

    while range.start < range.end {
        let mut buffer = vec![0u8; buffer_size(range.end - range.start)];
        match file.read(&mut buffer) {
            Ok(length) => {
                range.start += u64::from_usize(length).unwrap();
                buffer.truncate(length);
                let data = Bytes::from(buffer);
                sender.send(Ok(data)).await.unwrap();
            }
            Err(error) => {
                sender.send(Err(error)).await.unwrap();
                return;
            }
        }
    }
}
