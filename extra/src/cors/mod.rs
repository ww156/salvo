//! [CORS]: https://developer.mozilla.org/en-US/docs/Web/HTTP/CORS
//!
//! # Example
//!
//! ```
//! use salvo_core::prelude::*;
//! use salvo_extra::cors::CorsHandler;
//!
//! let cors_handler = CorsHandler::builder()
//!     .with_allow_origin("https://salvo.rs")
//!     .with_allow_methods(vec!["GET", "POST", "DELETE"]).build();
//!
//! let router = Router::new().hoop(cors_handler).post(upload_file);
//! #[fn_handler]
//! async fn upload_file(res: &mut Response) {
//! }
//!
//! ```
//! If you want to allow any route:
//! ```
//! use salvo_core::prelude::*;
//! use salvo_extra::cors::CorsHandler;
//! let cors_handler = CorsHandler::builder()
//!     .with_allow_any_origin().build();
//! ```

use std::collections::HashSet;
use std::convert::TryFrom;
use std::error::Error as StdError;

use async_trait::async_trait;
use salvo_core::http::header::{self, HeaderMap, HeaderName, HeaderValue};
use salvo_core::http::headers::{
    AccessControlAllowHeaders, AccessControlAllowMethods, AccessControlExposeHeaders, HeaderMapExt, Origin,
};
use salvo_core::http::{Method, Request, Response, StatusCode};
use salvo_core::routing::FlowCtrl;
use salvo_core::{Depot, Handler};

/// A constructed via `salvo_extra::cors::CorsHandler::builder()`.
#[derive(Clone, Debug)]
pub struct HandlerBuilder {
    credentials: bool,
    allowed_headers: HashSet<HeaderName>,
    exposed_headers: HashSet<HeaderName>,
    max_age: Option<u64>,
    methods: HashSet<Method>,
    origins: Option<HashSet<HeaderValue>>,
}
impl Default for HandlerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl HandlerBuilder {
    /// Create new `HandlerBuilder`.
    pub fn new() -> Self {
        HandlerBuilder {
            credentials: false,
            allowed_headers: HashSet::new(),
            exposed_headers: HashSet::new(),
            max_age: None,
            methods: HashSet::new(),
            origins: None,
        }
    }
    /// Sets whether to add the `Access-Control-Allow-Credentials` header.
    #[inline]
    pub fn with_allow_credentials(mut self, allow: bool) -> Self {
        self.credentials = allow;
        self
    }

    /// Adds a method to the existing list of allowed request methods.
    ///
    /// # Panics
    ///
    /// Panics if the provided argument is not a valid `http::Method`.
    #[inline]
    pub fn with_allow_method<M>(mut self, method: M) -> Self
    where
        Method: TryFrom<M>,
    {
        let method = match TryFrom::try_from(method) {
            Ok(m) => m,
            Err(_) => panic!("illegal Method"),
        };
        self.methods.insert(method);
        self
    }

    /// Adds multiple methods to the existing list of allowed request methods.
    ///
    /// # Panics
    ///
    /// Panics if the provided argument is not a valid `http::Method`.
    #[inline]
    pub fn with_allow_methods<I>(mut self, methods: I) -> Self
    where
        I: IntoIterator,
        Method: TryFrom<I::Item>,
    {
        let iter = methods.into_iter().map(|m| match TryFrom::try_from(m) {
            Ok(m) => m,
            Err(_) => panic!("illegal Method"),
        });
        self.methods.extend(iter);
        self
    }

    /// Adds a header to the list of allowed request headers.
    ///
    /// **Note**: These should match the values the browser sends via `Access-Control-Request-Headers`, e.g. `content-type`.
    ///
    /// # Panics
    ///
    /// Panics if the provided argument is not a valid `http::header::HeaderName`.
    #[inline]
    pub fn with_allow_header<H>(mut self, header: H) -> Self
    where
        HeaderName: TryFrom<H>,
    {
        let header = match TryFrom::try_from(header) {
            Ok(m) => m,
            Err(_) => panic!("illegal Header"),
        };
        self.allowed_headers.insert(header);
        self
    }

    /// Adds multiple headers to the list of allowed request headers.
    ///
    /// **Note**: These should match the values the browser sends via `Access-Control-Request-Headers`, e.g.`content-type`.
    ///
    /// # Panics
    ///
    /// Panics if any of the headers are not a valid `http::header::HeaderName`.
    #[inline]
    pub fn with_allow_headers<I>(mut self, headers: I) -> Self
    where
        I: IntoIterator,
        HeaderName: TryFrom<I::Item>,
    {
        let iter = headers.into_iter().map(|h| match TryFrom::try_from(h) {
            Ok(h) => h,
            Err(_) => panic!("illegal Header"),
        });
        self.allowed_headers.extend(iter);
        self
    }

    /// Adds a header to the list of exposed headers.
    ///
    /// # Panics
    ///
    /// Panics if the provided argument is not a valid `http::header::HeaderName`.
    #[inline]
    pub fn with_expose_header<H>(mut self, header: H) -> Self
    where
        HeaderName: TryFrom<H>,
    {
        let header = match TryFrom::try_from(header) {
            Ok(m) => m,
            Err(_) => panic!("illegal Header"),
        };
        self.exposed_headers.insert(header);
        self
    }

    /// Adds multiple headers to the list of exposed headers.
    ///
    /// # Panics
    ///
    /// Panics if any of the headers are not a valid `http::header::HeaderName`.
    #[inline]
    pub fn with_expose_headers<I>(mut self, headers: I) -> Self
    where
        I: IntoIterator,
        HeaderName: TryFrom<I::Item>,
    {
        let iter = headers.into_iter().map(|h| match TryFrom::try_from(h) {
            Ok(h) => h,
            Err(_) => panic!("illegal Header"),
        });
        self.exposed_headers.extend(iter);
        self
    }

    /// Sets that *any* `Origin` header is allowed.
    ///
    /// # Warning
    ///
    /// This can allow websites you didn't intend to access this resource,
    /// it is usually better to set an explicit list.
    #[inline]
    pub fn with_allow_any_origin(mut self) -> Self {
        self.origins = None;
        self
    }

    /// Add an origin to the existing list of allowed `Origin`s.
    ///
    /// # Panics
    ///
    /// Panics if the provided argument is not a valid `Origin`.
    #[inline]
    pub fn with_allow_origin(self, origin: impl IntoOrigin) -> Self {
        self.with_allow_origins(Some(origin))
    }

    /// Add multiple origins to the existing list of allowed `Origin`s.
    ///
    /// # Panics
    ///
    /// Panics if the provided argument is not a valid `Origin`.
    #[inline]
    pub fn with_allow_origins<I>(mut self, origins: I) -> Self
    where
        I: IntoIterator,
        I::Item: IntoOrigin,
    {
        let iter = origins.into_iter().map(IntoOrigin::into_origin).map(|origin| {
            origin
                .to_string()
                .parse()
                .expect("Origin is always a valid HeaderValue")
        });

        self.origins.get_or_insert_with(HashSet::new).extend(iter);

        self
    }

    /// Sets the `Access-Control-Max-Age` header.
    ///
    /// # Example
    ///
    ///
    /// ```
    /// use std::time::Duration;
    /// use salvo_core::prelude::*;;
    ///
    /// let cors = salvo_extra::cors::CorsHandler::builder()
    ///     .with_max_age(30) // 30u32 seconds
    ///     .with_max_age(Duration::from_secs(30)); // or a Duration
    /// ```
    #[inline]
    pub fn with_max_age(mut self, seconds: impl Seconds) -> Self {
        self.max_age = Some(seconds.seconds());
        self
    }

    /// Builds the `Cors` wrapper from the configured settings.
    ///
    /// This step isn't *required*, as the `HandlerBuilder` itself can be passed
    /// to `Filter::with`. This just allows constructing once, thus not needing
    /// to pay the cost of "building" every time.
    pub fn build(self) -> CorsHandler {
        let expose_headers_header = if self.exposed_headers.is_empty() {
            None
        } else {
            Some(self.exposed_headers.iter().cloned().collect())
        };
        let allowed_headers_header = self.allowed_headers.iter().cloned().collect();
        let methods_header = self.methods.iter().cloned().collect();

        let HandlerBuilder {
            credentials,
            allowed_headers,
            // exposed_headers,
            max_age,
            methods,
            origins,
            ..
        } = self;

        CorsHandler {
            credentials,
            allowed_headers,
            // exposed_headers,
            max_age,
            methods,
            origins,
            allowed_headers_header,
            expose_headers_header,
            methods_header,
        }
    }
}

enum Forbidden {
    Origin,
    Method,
    Header,
}

impl ::std::fmt::Debug for Forbidden {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        f.debug_tuple("CorsForbidden").field(&self).finish()
    }
}

impl ::std::fmt::Display for Forbidden {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        let detail = match self {
            Forbidden::Origin => "origin not allowed",
            Forbidden::Method => "request-method not allowed",
            Forbidden::Header => "header not allowed",
        };
        write!(f, "CORS request forbidden: {}", detail)
    }
}

impl StdError for Forbidden {}

enum Validated {
    Preflight(HeaderValue),
    Simple(HeaderValue),
    NotCors,
}

/// CorsHandler
#[derive(Debug)]
pub struct CorsHandler {
    credentials: bool,
    allowed_headers: HashSet<HeaderName>,
    // exposed_headers: HashSet<HeaderName>,
    max_age: Option<u64>,
    methods: HashSet<Method>,
    origins: Option<HashSet<HeaderValue>>,
    allowed_headers_header: AccessControlAllowHeaders,
    expose_headers_header: Option<AccessControlExposeHeaders>,
    methods_header: AccessControlAllowMethods,
}
impl CorsHandler {
    /// Returns `HandlerBuilder` instance for build `CorsHandler`.
    #[inline]
    pub fn builder() -> HandlerBuilder {
        HandlerBuilder::default()
    }
    fn check_request(&self, method: &Method, headers: &HeaderMap) -> Result<Validated, Forbidden> {
        match (headers.get(header::ORIGIN), method) {
            (Some(origin), &Method::OPTIONS) => {
                // OPTIONS requests are preflight CORS requests...
                if !self.is_origin_allowed(origin) {
                    return Err(Forbidden::Origin);
                }

                if let Some(req_method) = headers.get(header::ACCESS_CONTROL_REQUEST_METHOD) {
                    if !self.is_method_allowed(req_method) {
                        return Err(Forbidden::Method);
                    }
                } else {
                    tracing::debug!("preflight request missing access-control-request-method header");
                    return Err(Forbidden::Method);
                }

                if let Some(req_headers) = headers.get(header::ACCESS_CONTROL_REQUEST_HEADERS) {
                    let headers = req_headers.to_str().map_err(|_| Forbidden::Header)?;
                    for header in headers.split(',') {
                        if !self.is_header_allowed(header) {
                            return Err(Forbidden::Header);
                        }
                    }
                }

                Ok(Validated::Preflight(origin.clone()))
            }
            (Some(origin), _) => {
                // Any other method, simply check for a valid origin...
                tracing::debug!("origin header: {:?}", origin);
                if self.is_origin_allowed(origin) {
                    Ok(Validated::Simple(origin.clone()))
                } else {
                    Err(Forbidden::Origin)
                }
            }
            (None, _) => {
                // No `ORIGIN` header means this isn't CORS!
                Ok(Validated::NotCors)
            }
        }
    }

    #[inline]
    fn is_method_allowed(&self, header: &HeaderValue) -> bool {
        Method::from_bytes(header.as_bytes())
            .map(|method| self.methods.contains(&method))
            .unwrap_or(false)
    }

    #[inline]
    fn is_header_allowed(&self, header: &str) -> bool {
        HeaderName::from_bytes(header.as_bytes())
            .map(|header| self.allowed_headers.contains(&header))
            .unwrap_or(false)
    }

    #[inline]
    fn is_origin_allowed(&self, origin: &HeaderValue) -> bool {
        if let Some(ref allowed) = self.origins {
            allowed.contains(origin)
        } else {
            true
        }
    }

    fn append_preflight_headers(&self, headers: &mut HeaderMap) {
        self.append_common_headers(headers);

        headers.typed_insert(self.allowed_headers_header.clone());
        headers.typed_insert(self.methods_header.clone());

        if let Some(max_age) = self.max_age {
            headers.insert(header::ACCESS_CONTROL_MAX_AGE, max_age.into());
        }
    }

    fn append_common_headers(&self, headers: &mut HeaderMap) {
        if self.credentials {
            headers.insert(
                header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
                HeaderValue::from_static("true"),
            );
        }
        if let Some(expose_headers_header) = &self.expose_headers_header {
            headers.typed_insert(expose_headers_header.clone())
        }
    }
}

#[async_trait]
impl Handler for CorsHandler {
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        let validated = self.check_request(req.method(), req.headers());

        match validated {
            Ok(Validated::Preflight(origin)) => {
                self.append_preflight_headers(res.headers_mut());
                res.headers_mut().insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
                ctrl.call_next(req, depot, res).await;
            }
            Ok(Validated::Simple(origin)) => {
                self.append_common_headers(res.headers_mut());
                res.headers_mut().insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
                ctrl.call_next(req, depot, res).await;
            }
            Err(err) => {
                tracing::error!(error = %err, "CorsHandler validate error");
                res.set_status_code(StatusCode::FORBIDDEN);
                ctrl.skip_reset();
            }
            _ => {
                ctrl.call_next(req, depot, res).await;
            }
        }
    }
}

/// Seconds
pub trait Seconds {
    /// Get seconds.
    fn seconds(self) -> u64;
}

impl Seconds for u32 {
    #[inline]
    fn seconds(self) -> u64 {
        self.into()
    }
}

impl Seconds for ::std::time::Duration {
    #[inline]
    fn seconds(self) -> u64 {
        self.as_secs()
    }
}

/// IntoOrigin
pub trait IntoOrigin {
    /// Convert to `Origin`.
    fn into_origin(self) -> Origin;
}

impl<'a> IntoOrigin for &'a str {
    fn into_origin(self) -> Origin {
        let mut parts = self.splitn(2, "://");
        let scheme = parts.next().expect("missing scheme");
        let rest = parts.next().expect("missing scheme");

        Origin::try_from_parts(scheme, rest, None).expect("invalid Origin")
    }
}

#[cfg(test)]
mod tests {
    use salvo_core::http::header::*;
    use salvo_core::hyper;
    use salvo_core::prelude::*;

    use super::*;

    #[tokio::test]
    async fn test_cors() {
        let cors_handler = CorsHandler::builder()
            .with_allow_origin("https://salvo.rs")
            .with_allow_methods(vec!["GET", "POST", "OPTIONS"])
            .with_allow_headers(vec![
                "CONTENT-TYPE",
                "Access-Control-Request-Method",
                "Access-Control-Allow-Origin",
                "Access-Control-Allow-Headers",
                "Access-Control-Max-Age",
            ])
            .build();

        #[fn_handler]
        async fn hello() -> &'static str {
            "hello"
        }

        let router = Router::new()
            .hoop(cors_handler)
            .push(Router::with_path("hello").handle(hello));
        let service = Service::new(router);

        async fn options_access(service: &Service, origin: &str) -> Response {
            let req: Request = hyper::Request::builder()
                .method("OPTIONS")
                .uri("http://127.0.0.1:7979/hello")
                .header("Origin", origin)
                .header("Access-Control-Request-Method", "POST")
                .header("Access-Control-Request-Headers", "Content-Type")
                .body(hyper::Body::empty())
                .unwrap()
                .into();
            service.handle(req).await
        }

        async fn access(service: &Service, method: &str, origin: &str) -> Response {
            let req: Request = hyper::Request::builder()
                .method(method)
                .uri("http://127.0.0.1:7979/hello")
                .header("Origin", origin)
                .body(hyper::Body::empty())
                .unwrap()
                .into();
            service.handle(req).await
        }

        let res = access(&service, "OPTIONS", "https://salvo.rs").await;
        let headers = res.headers();
        assert!(headers.get(ACCESS_CONTROL_ALLOW_METHODS).is_none());

        let res = options_access(&service, "https://salvo.rs").await;
        let headers = res.headers();
        assert!(headers.get(ACCESS_CONTROL_ALLOW_METHODS).is_some());
        assert!(headers.get(ACCESS_CONTROL_ALLOW_HEADERS).is_some());

        let res = access(&service, "OPTIONS", "https://google.com").await;
        let headers = res.headers();
        assert!(
            headers.get(ACCESS_CONTROL_ALLOW_METHODS).is_none(),
            "POST, GET, DELETE, OPTIONS"
        );
        assert!(headers.get(ACCESS_CONTROL_ALLOW_HEADERS).is_none());

        let content = access(&service, "GET", "https://salvo.rs")
            .await
            .take_text()
            .await
            .unwrap();
        assert!(content.contains("hello"));

        let content = access(&service, "GET", "https://google.rs")
            .await
            .take_text()
            .await
            .unwrap();
        assert!(content.contains("Forbidden"));
    }
}
