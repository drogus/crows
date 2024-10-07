use anyhow::Result;
use crows_utils::services::RequestInfo;
use http_body_util::BodyExt;
use hyper_rustls::ConfigBuilderExt;
use hyper::body::{Body, Incoming as IncomingBody};
use hyper::{Request, Response};
use hyper_util::rt::{TokioExecutor, TokioIo};
use rustls::pki_types::ServerName;
use rustls::ClientConfig;
use std::collections::HashMap;
use std::net::ToSocketAddrs;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use url::Url;

// I'm not a big fan of doing this, but it looks like the easiest way to measure
// things now.
//
// We need to measure stuff like bytes_sent or send time and it's not easily
// obtainable from hyper APIs. Thus, in order to be able to get these, we create
// a proxy between hyper and the TcpStream that we open for the connection.
struct MeasuringStream<T> {
    inner: T,
    bytes_sent: Arc<AtomicU64>,
    bytes_received: Arc<AtomicU64>,
    time_to_first_byte: Arc<AtomicU64>,
    send_time: Arc<AtomicU64>,
    start_time: Instant,
}

impl<T: AsyncRead + AsyncWrite + Unpin> AsyncRead for MeasuringStream<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let result = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = result {
            let bytes_read = buf.filled().len() as u64;
            self.bytes_received.fetch_add(bytes_read, Ordering::SeqCst);

            if self.time_to_first_byte.load(Ordering::SeqCst) == 0 && bytes_read > 0 {
                self.time_to_first_byte.store(
                    self.start_time.elapsed().as_micros() as u64,
                    Ordering::SeqCst,
                )
            }
        }
        result
    }
}

impl<T: AsyncRead + AsyncWrite + Unpin> AsyncWrite for MeasuringStream<T> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        let result = Pin::new(&mut self.inner).poll_write(cx, buf);
        if let Poll::Ready(Ok(bytes_written)) = result {
            self.bytes_sent
                .fetch_add(bytes_written as u64, Ordering::SeqCst);
            // this is not greatas we update the send time each time we write anything
            // the last update should be the actual send_time
            self.send_time.store(
                self.start_time.elapsed().as_micros() as u64,
                Ordering::SeqCst,
            )
        }
        result
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

pub struct Client {
    tls_config: Arc<ClientConfig>,
    connections: HashMap<String, SendRequest<String>>,
}

#[derive(Debug)]
struct SendRequest<B> {
    sender: SendRequestInner<B>,
    bytes_sent: Arc<AtomicU64>,
    bytes_received: Arc<AtomicU64>,
    time_to_first_byte: Arc<AtomicU64>,
    send_time: Arc<AtomicU64>,
    tls_handshake_time: Option<Duration>,
    connecting_time: Duration,
}

#[derive(Debug)]
enum SendRequestInner<B> {
    Http1SendRequest(hyper::client::conn::http1::SendRequest<B>),
    Http2SendRequest(hyper::client::conn::http2::SendRequest<B>),
}

impl<B: Body + 'static> SendRequest<B> {
    async fn send_request(&mut self, req: Request<B>) -> hyper::Result<Response<IncomingBody>> {
        match &mut self.sender {
            SendRequestInner::Http1SendRequest(h) => h.send_request(req).await,
            SendRequestInner::Http2SendRequest(h) => h.send_request(req).await,
        }
    }

    fn is_closed(&self) -> bool {
        match &self.sender {
            SendRequestInner::Http1SendRequest(h) => h.is_closed(),
            SendRequestInner::Http2SendRequest(h) => h.is_closed(),
        }
    }

    fn reset(&mut self) {
        self.bytes_sent.store(0, Ordering::SeqCst);
        self.bytes_received.store(0, Ordering::SeqCst);
        self.time_to_first_byte.store(0, Ordering::SeqCst);
    }
}

impl Client {
    pub fn new(tls_config: Arc<ClientConfig>) -> Self {
        Self {
            tls_config,
            connections: HashMap::new(),
        }
    }

    pub async fn http_request(
        &mut self,
        request: http::Request,
    ) -> Result<(http::Response, RequestInfo)> {
        let start = Instant::now();
        let url = Url::parse(request.uri().as_str()).map_err(|err| anyhow::anyhow!("Error when parsing the URL: {err:?}"))?;

        let host = url.host_str().ok_or_else(|| anyhow::anyhow!("No host in URL"))?;
        let port = url.port_or_known_default().ok_or_else(|| anyhow::anyhow!("Unable to determine port"))?;

        let mut tls_handshake_time = None;
        let mut connecting_time = None;

        let connection_key = format!("{}://{}:{}", url.scheme(), host, port);
        let sender = match self.connections.get_mut(&connection_key) {
            Some(sender) if !sender.is_closed() => sender,
            _ => {
                let sender = self.create_connection(&url, host, port).await?;
                tls_handshake_time = sender.tls_handshake_time;
                connecting_time = Some(sender.connecting_time);
                self.connections.insert(connection_key.clone(), sender);
                self.connections.get_mut(&connection_key).unwrap()
            }
        };

        let mut req_builder = Request::builder()
            .method(request.method().as_str())
            .uri(request.uri().as_str());

        req_builder = req_builder.header("host", host);
        for (key, value) in request.headers().iter() {
            req_builder = req_builder.header(key.as_str(), value.as_str());
        }

        let body = request.body().unwrap_or_default();
        let req = req_builder.body(body.to_vec()).map_err(|err| anyhow::anyhow!("Failed to build request: {err:?}"))?;

        let send_start = Instant::now();
        let res = sender.send_request(req).await.map_err(|err| anyhow::anyhow!("Failed to send request: {err:?}"))?;
        let latency = send_start.elapsed();

        let status = res.status().as_u16();
        let successful = res.status().is_success();

        let mut headers = http::Headers::new();
        for (name, value) in res.headers() {
            headers.append(name.as_str(), value.to_str().map_err(|err| anyhow::anyhow!("Could not parse response header {value:?}: {err:?}"))?);
        }

        let body = res.into_body();
        let body = body
            .collect()
            .await
            .map_err(|err| anyhow::anyhow!("Failed to collect response body: {err:?}"))?
            .to_bytes();

        let total_time = start.elapsed();

        let request_info = RequestInfo {
            latency: total_time,
            successful,
            status,
            tls_handshake_time,
            connecting_time,
            response_time: latency,
            bytes_sent: sender.bytes_sent.load(Ordering::SeqCst),
            bytes_received: sender.bytes_received.load(Ordering::SeqCst),
            time_to_first_byte: Duration::from_micros(
                sender.time_to_first_byte.load(Ordering::SeqCst),
            ),
            send_time: Duration::from_micros(sender.send_time.load(Ordering::SeqCst)),
        };

        sender.reset();

        let http_response = http::Response::new(status)
            .with_headers(headers)
            .with_body(Some(body.to_vec()));

        Ok((http_response, request_info))
    }

    async fn create_connection(
        &self,
        url: &Url,
        host: &str,
        port: u16,
    ) -> Result<SendRequest<String>, HTTPError> {
        let server_name = ServerName::try_from(host.to_string()).map_err(|_| HTTPError {
            message: format!("Invalid DNS name: {}", host),
        })?;

        let addr = format!("{}:{}", host, port);
        // DNS resolution is a blocking operation in tokio
        let socket_addr = tokio::task::spawn_blocking(move || addr.to_socket_addrs()).await
            .map_err(|err| HTTPError {
                message: format!("Failed to resolve domain: {err:?}"),
            })?
            .map_err(|err| HTTPError {
                message: format!("Failed to resolve domain: {err:?}"),
            })?
            .next()
            .ok_or_else(|| HTTPError {
                message: "No valid socket address found for the domain".to_string(),
            })?;

        let connect_start = Instant::now();
        let tcp_stream = TcpStream::connect(socket_addr)
            .await
            .map_err(|err| HTTPError {
                message: format!("Failed to connect: {err:?}"),
            })?;
        let connecting_time = connect_start.elapsed();

        let bytes_sent = Arc::new(AtomicU64::new(0));
        let bytes_received = Arc::new(AtomicU64::new(0));
        let time_to_first_byte = Arc::new(AtomicU64::new(0));
        let send_time = Arc::new(AtomicU64::new(0));

        let measuring_stream = MeasuringStream {
            inner: tcp_stream,
            bytes_sent: Arc::clone(&bytes_sent),
            bytes_received: Arc::clone(&bytes_received),
            time_to_first_byte: Arc::clone(&time_to_first_byte),
            send_time: Arc::clone(&send_time),
            start_time: Instant::now(),
        };

        let mut tls_handshake_time = None;
        let sender_inner = if url.scheme() == "https" {
            let tls_start = Instant::now();
            let connector = tokio_rustls::TlsConnector::from(self.tls_config.clone())
                .connect(server_name, measuring_stream)
                .await
                .map_err(|err| HTTPError {
                    message: format!("TLS connection failed: {err:?}"),
                })?;
            let io = TokioIo::new(connector);
            tls_handshake_time = Some(tls_start.elapsed());

            let (_tcp, tls) = io.inner().get_ref();
            if tls.alpn_protocol() == Some(b"h2") {
                let (sender, conn) =
                    hyper::client::conn::http2::handshake(TokioExecutor::new(), io)
                        .await
                        .map_err(|err| HTTPError {
                            message: format!("Failed to create HTTP connection: {err:?}"),
                        })?;
                tokio::task::spawn(async move {
                    if let Err(err) = conn.await {
                        println!("Connection failed: {:?}", err);
                    }
                });
                SendRequestInner::Http2SendRequest(sender)
            } else {
                let (sender, conn) =
                    hyper::client::conn::http1::handshake(io)
                        .await
                        .map_err(|err| HTTPError {
                            message: format!("Failed to create HTTP connection: {err:?}"),
                        })?;
                tokio::task::spawn(async move {
                    if let Err(err) = conn.await {
                        println!("Connection failed: {:?}", err);
                    }
                });
                SendRequestInner::Http1SendRequest(sender)
            }
        } else {
            let io = TokioIo::new(measuring_stream);
            let (sender, conn) =
                hyper::client::conn::http1::handshake(io)
                    .await
                    .map_err(|err| HTTPError {
                        message: format!("Failed to create HTTP connection: {err:?}"),
                    })?;
            tokio::task::spawn(async move {
                if let Err(err) = conn.await {
                    println!("Connection failed: {:?}", err);
                }
            });
            SendRequestInner::Http1SendRequest(sender)
        };

        Ok(SendRequest {
            sender: sender_inner,
            bytes_sent,
            bytes_received,
            send_time,
            time_to_first_byte,
            tls_handshake_time,
            connecting_time,
        })
    }

    pub fn clear_connections(&mut self) {
        self.connections.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_http_request() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/test"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("Hello, World!")
                    .insert_header("content-type", "text/plain"),
            )
            .mount(&mock_server)
            .await;

        let request = HTTPRequest {
            url: format!("{}/test", mock_server.uri()),
            method: HTTPMethod::GET,
            headers: HashMap::new(),
            body: None,
        };

        let mut client = Client::new(Arc::new(
            rustls::ClientConfig::builder()
                .with_root_certificates(rustls::RootCertStore::empty())
                .with_no_client_auth(),
        ));
        let result = client.http_request(request).await;
        assert!(result.is_ok());

        let (response, request_info) = result.unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(String::from_utf8(response.body).unwrap(), "Hello, World!");
        assert!(response.headers.contains_key("content-type"));
        assert_eq!(response.headers["content-type"], "text/plain");

        assert!(request_info.successful);
        assert_eq!(request_info.status, 200);
        assert!(request_info.latency > Duration::from_nanos(0));
        assert!(request_info.response_time > Duration::from_nanos(0));
    }

    #[tokio::test]
    async fn test_https_request() {
        // I don't like the fact that this test relies on having internet access,
        // but I don't have time to setup sth better that would work locally
        // TODO: fix this
        rustls::crypto::ring::default_provider()
            .install_default()
            .unwrap();
        let request = HTTPRequest {
            url: "https://example.org".to_string(),
            method: HTTPMethod::GET,
            headers: HashMap::new(),
            body: None,
        };

        let mut client = Client::new(Arc::new(
            rustls::ClientConfig::builder()
                .with_native_roots()
                .unwrap()
                .with_no_client_auth(),
        ));
        let result = client.http_request(request).await;
        assert!(result.is_ok());

        let (response, request_info) = result.unwrap();
        assert_eq!(response.status, 200);

        assert!(request_info.successful);
        assert_eq!(request_info.status, 200);
        assert!(request_info.latency > Duration::from_nanos(0));
        assert!(request_info.response_time > Duration::from_nanos(0));
        assert!(request_info.tls_handshake_time.unwrap() > Duration::from_nanos(0));
    }
}
