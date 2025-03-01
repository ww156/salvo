//! Http

pub mod errors;
pub mod form;
mod range;
pub mod request;
pub mod response;

pub use cookie;
pub use errors::{HttpError, ReadError};
pub use headers;
pub use http::method::Method;
pub use http::{header, method, uri, version, HeaderMap, HeaderValue, StatusCode};
pub use hyper::body::HttpBody;
pub use mime::Mime;
pub use range::HttpRange;
pub use request::Request;
pub use response::Response;

pub(crate) fn guess_accept_mime(req: &Request, default_type: Option<Mime>) -> Mime {
    let dmime: Mime = default_type.unwrap_or_else(|| "text/html".parse().unwrap());
    let accept = req.accept();
    accept.first().unwrap_or(&dmime).to_string().parse().unwrap_or(dmime)
}

#[cfg(test)]
mod tests {
    use super::header::*;
    use super::*;

    #[test]
    fn test_guess_accept_mime() {
        let mut req = Request::default();
        let headers = req.headers_mut();
        headers.insert(ACCEPT, HeaderValue::from_static("application/javascript"));
        let mime = guess_accept_mime(&req, None);
        assert_eq!(mime, "application/javascript".parse::<Mime>().unwrap());
    }
}
