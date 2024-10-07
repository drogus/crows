use std::any::Any;

use crows_utils::services::RequestInfo;
use hyper::HeaderMap;
use tokio::sync::mpsc::UnboundedSender;
use wasmtime::{component::bindgen, Caller, Memory, Store};
pub struct OutgoingBody;
pub struct ResponseOutparam;
pub struct RequestOptions;
pub struct OutgoingResponse;

#[derive(Debug)]
pub enum Fields {
    /// A reference to the fields of a parent entry.
    Ref {
        /// The parent resource rep.
        parent: u32,

        /// The function to get the fields from the parent.
        // NOTE: there's not failure in the result here because we assume that HostFields will
        // always be registered as a child of the entry with the `parent` id. This ensures that the
        // entry will always exist while this `HostFields::Ref` entry exists in the table, thus we
        // don't need to account for failure when fetching the fields ref from the parent.
        get_fields: for<'a> fn(elem: &'a mut (dyn Any + 'static)) -> &'a mut HeaderMap,
    },
    /// An owned version of the fields.
    Owned {
        /// The fields themselves.
        fields: HeaderMap,
    },
}

#[allow(missing_docs)]
mod generated {
    pub use super::{Fields,OutgoingBody,OutgoingResponse,RequestOptions,ResponseOutparam};

    wasmtime::component::bindgen!({
        path: "wit",
        world: "crows",
        async: true,
        trappable_imports: true,
        require_store_data_send: true,
        with: {
            "wasi:io": wasmtime_wasi::bindings::io,

            "wasi:http/types/outgoing-body": OutgoingBody,
            "wasi:http/types/outgoing-response": OutgoingResponse,
            "wasi:http/types/response-outparam": ResponseOutparam,
            "wasi:http/types/fields": Fields,
            "wasi:http/types/request-options": RequestOptions,
        },
        trappable_error_type: {
            "wasi:http/types/error-code" => crate::http::HTTPError,
        },
    });
}

pub use generated::{CrowsPre, Crows};

pub use self::generated::wasi::*;

/// Raw bindings to the `wasi:http/proxy` exports.
pub use self::generated::exports;

pub struct HostComponent {
    client: Client,
    request_info_sender: UnboundedSender<RequestInfo>,
}

use crate::http_client::Client;

impl HostComponent {
    pub fn new(client: Client, request_info_sender: UnboundedSender<RequestInfo>) -> Self {
        Self {
            client,
            request_info_sender,
        }
    }

    pub fn clear_connections(&mut self) {
        self.client.clear_connections();
    }
}

// #[async_trait::async_trait]
// impl outgoing_handler::Host for HostComponent {
//     async fn handle(&mut self, request: http::Request) -> Result<http::Response, http::Error> {
//         let (http_response, request_info) = self
//             .client
//             .http_request(request)
//             .await
//             .map_err(|e| http::Error::InvalidUrl(e.to_string()))?;
//
//         let _ = self.request_info_sender.send(request_info);
//
//         Ok(http_response)
//     }
// }
#[async_trait::async_trait]
impl outgoing_handler::Host for HostComponent {
    fn handle(
        &mut self,
        request_id: Resource<HostOutgoingRequest>,
        options: Option<Resource<types::RequestOptions>>,
    ) -> crate::HttpResult<Resource<HostFutureIncomingResponse>> {
        let opts = options.and_then(|opts| self.table().get(&opts).ok());

        let connect_timeout = opts
            .and_then(|opts| opts.connect_timeout)
            .unwrap_or(std::time::Duration::from_secs(600));

        let first_byte_timeout = opts
            .and_then(|opts| opts.first_byte_timeout)
            .unwrap_or(std::time::Duration::from_secs(600));

        let between_bytes_timeout = opts
            .and_then(|opts| opts.between_bytes_timeout)
            .unwrap_or(std::time::Duration::from_secs(600));

        let req = self.table().delete(request_id)?;
        let mut builder = hyper::Request::builder();

        builder = builder.method(match req.method {
            types::Method::Get => Method::GET,
            types::Method::Head => Method::HEAD,
            types::Method::Post => Method::POST,
            types::Method::Put => Method::PUT,
            types::Method::Delete => Method::DELETE,
            types::Method::Connect => Method::CONNECT,
            types::Method::Options => Method::OPTIONS,
            types::Method::Trace => Method::TRACE,
            types::Method::Patch => Method::PATCH,
            types::Method::Other(m) => match hyper::Method::from_bytes(m.as_bytes()) {
                Ok(method) => method,
                Err(_) => return Err(types::ErrorCode::HttpRequestMethodInvalid.into()),
            },
        });

        let (use_tls, scheme) = match req.scheme.unwrap_or(Scheme::Https) {
            Scheme::Http => (false, http::uri::Scheme::HTTP),
            Scheme::Https => (true, http::uri::Scheme::HTTPS),

            // We can only support http/https
            Scheme::Other(_) => return Err(types::ErrorCode::HttpProtocolError.into()),
        };

        let authority = req.authority.unwrap_or_else(String::new);

        builder = builder.header(hyper::header::HOST, &authority);

        let mut uri = http::Uri::builder()
            .scheme(scheme)
            .authority(authority.clone());

        if let Some(path) = req.path_with_query {
            uri = uri.path_and_query(path);
        }

        builder = builder.uri(uri.build().map_err(http_request_error)?);

        for (k, v) in req.headers.iter() {
            builder = builder.header(k, v);
        }

        let body = req.body.unwrap_or_else(|| {
            Empty::<Bytes>::new()
                .map_err(|_| unreachable!("Infallible error"))
                .boxed()
        });

        let request = builder
            .body(body)
            .map_err(|err| internal_error(err.to_string()))?;

        let future = self.send_request(
            request,
            OutgoingRequestConfig {
                use_tls,
                connect_timeout,
                first_byte_timeout,
                between_bytes_timeout,
            },
        )?;

        Ok(self.table().push(future)?)
    }
}


