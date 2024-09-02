use std::{any::Any, io::IoSlice};
use tokio::sync::mpsc::UnboundedSender;
use wasi_common::file::{FdFlags, FileType};
use wasi_common::WasiFile;
use wasmtime_wasi::{HostOutputStream, StdoutStream, StreamResult};

#[derive(Clone)]
pub struct RemoteIo {
    pub sender: UnboundedSender<Vec<u8>>,
}

#[wiggle::async_trait]
impl WasiFile for RemoteIo {
    fn as_any(&self) -> &dyn Any {
        self
    }
    async fn get_filetype(&self) -> Result<FileType, wasi_common::Error> {
        Ok(FileType::Pipe)
    }
    async fn get_fdflags(&self) -> Result<FdFlags, wasi_common::Error> {
        Ok(FdFlags::APPEND)
    }
    async fn write_vectored<'a>(&self, bufs: &[IoSlice<'a>]) -> Result<u64, wasi_common::Error> {
        let mut size: u64 = 0;
        for slice in bufs {
            let slice = slice.to_vec();
            size += slice.len() as u64;
            self.sender.send(slice).unwrap();
        }
        Ok(size)
    }
}

impl HostOutputStream for RemoteIo {
    fn write(&mut self, bytes: bytes::Bytes) -> StreamResult<()> {
        self.sender.send(bytes.to_vec()).unwrap();

        Ok(())
    }

    fn flush(&mut self) -> StreamResult<()> {
        Ok(())
    }

    fn check_write(&mut self) -> StreamResult<usize> {
        Ok(1024 * 1024)
    }
}

impl StdoutStream for RemoteIo {
    fn stream(&self) -> Box<dyn HostOutputStream> {
        Box::new(self.clone())
    }

    fn isatty(&self) -> bool {
        false
    }
}

#[async_trait::async_trait]
impl wasmtime_wasi::Subscribe for RemoteIo {
    async fn ready(&mut self) {}
}
