use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::AsyncWrite;
use tokio::sync::mpsc::UnboundedSender;
use wasmtime_wasi::cli::{IsTerminal, StdoutStream};

#[derive(Clone)]
pub struct RemoteIo {
    pub sender: UnboundedSender<Vec<u8>>,
}

impl IsTerminal for RemoteIo {
    fn is_terminal(&self) -> bool {
        false
    }
}

impl StdoutStream for RemoteIo {
    fn async_stream(&self) -> Box<dyn AsyncWrite + Send + Sync> {
        Box::new(RemoteIoWriter {
            sender: self.sender.clone(),
        })
    }
}

struct RemoteIoWriter {
    sender: UnboundedSender<Vec<u8>>,
}

impl AsyncWrite for RemoteIoWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let len = buf.len();
        if let Err(_) = self.sender.send(buf.to_vec()) {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "receiver dropped",
            )));
        }
        Poll::Ready(Ok(len))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
