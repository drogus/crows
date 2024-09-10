pub struct RequestInfo {
    pub latency: Duration,
    pub successful: bool,
    pub dns_time: Duration,
    pub tcp_connect_time: Duration,
    pub tls_handshake_time: Duration,
    pub request_write_time: Duration,
    pub response_time: Duration,
}
use std::time::Duration;
