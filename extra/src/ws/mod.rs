// Copyright (c) 2018-2020 Sean McArthur
// Licensed under the MIT license http://opensource.org/licenses/MIT
// port from https://github.com/seanmonstar/warp/blob/master/src/filters/ws.rs

//! Websocket

use std::borrow::Cow;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::sink::Sink;
use futures_util::stream::Stream;
use futures_util::{future, ready, FutureExt, TryFutureExt};
use hyper::upgrade::OnUpgrade;
use salvo_core::http::errors::*;
use salvo_core::http::header::{SEC_WEBSOCKET_VERSION, UPGRADE};
use salvo_core::http::headers::{Connection, HeaderMapExt, SecWebsocketAccept, SecWebsocketKey, Upgrade};
use salvo_core::http::{HttpError, StatusCode};
use salvo_core::{Error, Request, Response};
use tokio_tungstenite::{
    tungstenite::protocol::{self, WebSocketConfig},
    WebSocketStream,
};

/// Creates a Websocket Handler.
/// Request:
/// - Method must be `GET`
/// - Header `connection` must be `upgrade`
/// - Header `upgrade` must be `websocket`
/// - Header `sec-websocket-version` must be `13`
/// - Header `sec-websocket-key` must be set.
///
/// Response:
/// - Status of `101 Switching Protocols`
/// - Header `connection: upgrade`
/// - Header `upgrade: websocket`
/// - Header `sec-websocket-accept` with the hash value of the received key.
#[allow(missing_debug_implementations)]
pub struct WsHandler {
    config: Option<WebSocketConfig>,
}

impl Default for WsHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl WsHandler {
    /// Create new `WsHandler`.
    pub fn new() -> Self {
        WsHandler { config: None }
    }
    /// Create new `WsHandler` with config.
    pub fn with_config(config: WebSocketConfig) -> Self {
        WsHandler { config: Some(config) }
    }

    /// Set the size of the internal message send queue.
    pub fn max_send_queue(mut self, max: usize) -> Self {
        self.config.get_or_insert_with(WebSocketConfig::default).max_send_queue = Some(max);
        self
    }

    /// Set the maximum message size (defaults to 64 megabytes)
    pub fn max_message_size(mut self, max: usize) -> Self {
        self.config
            .get_or_insert_with(WebSocketConfig::default)
            .max_message_size = Some(max);
        self
    }

    /// Set the maximum frame size (defaults to 16 megabytes)
    pub fn max_frame_size(mut self, max: usize) -> Self {
        self.config.get_or_insert_with(WebSocketConfig::default).max_frame_size = Some(max);
        self
    }

    /// Handle websocket request.
    pub fn handle(
        &self,
        req: &mut Request,
        res: &mut Response,
    ) -> Result<impl Future<Output = Option<WebSocket>>, HttpError> {
        let req_headers = req.headers();
        let matched = req_headers
            .typed_get::<Connection>()
            .map(|conn| conn.contains(UPGRADE))
            .unwrap_or(false);
        if !matched {
            tracing::debug!("missing connection upgrade");
            return Err(BadRequest().with_summary("missing connection upgrade"));
        }
        let matched = req_headers
            .get(UPGRADE)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.to_lowercase() == "websocket")
            .unwrap_or(false);
        if !matched {
            tracing::debug!("missing upgrade header or it is not equal websocket");
            return Err(BadRequest().with_summary("missing upgrade header or it is not equal websocket"));
        }
        let matched = !req_headers
            .get(SEC_WEBSOCKET_VERSION)
            .and_then(|v| v.to_str().ok())
            .map(|v| v == "13")
            .unwrap_or(false);
        if matched {
            tracing::debug!("websocket version is not equal 13");
            return Err(BadRequest().with_summary("websocket version is not equal 13"));
        }
        let sec_ws_key = if let Some(key) = req_headers.typed_get::<SecWebsocketKey>() {
            key
        } else {
            tracing::debug!("sec_websocket_key is not exist in request headers");
            return Err(BadRequest().with_summary("sec_websocket_key is not exist in request headers"));
        };

        res.set_status_code(StatusCode::SWITCHING_PROTOCOLS);

        res.headers_mut().typed_insert(Connection::upgrade());
        res.headers_mut().typed_insert(Upgrade::websocket());
        res.headers_mut().typed_insert(SecWebsocketAccept::from(sec_ws_key));

        if let Some(on_upgrade) = req.extensions_mut().remove::<OnUpgrade>() {
            let config = self.config;
            let fut = async move {
                let ws = on_upgrade
                    .and_then(move |upgraded| {
                        tracing::debug!("websocket upgrade complete");
                        WebSocket::from_raw_socket(upgraded, protocol::Role::Server, config).map(Ok)
                    })
                    .await
                    .ok();
                ws
            };
            Ok(fut)
        } else {
            tracing::debug!("ws couldn't be upgraded since no upgrade state was present");
            Err(BadRequest().with_summary("ws couldn't be upgraded since no upgrade state was present"))
        }
    }
}

/// A websocket `Stream` and `Sink`, provided to `ws` filters.
///
/// Ping messages sent from the client will be handled internally by replying with a Pong message.
/// Close messages need to be handled explicitly: usually by closing the `Sink` end of the
/// `WebSocket`.
pub struct WebSocket {
    inner: WebSocketStream<hyper::upgrade::Upgraded>,
}

impl WebSocket {
    pub(crate) async fn from_raw_socket(
        upgraded: hyper::upgrade::Upgraded,
        role: protocol::Role,
        config: Option<protocol::WebSocketConfig>,
    ) -> Self {
        WebSocketStream::from_raw_socket(upgraded, role, config)
            .map(|inner| WebSocket { inner })
            .await
    }

    /// Gracefully close this websocket.
    pub async fn close(mut self) -> Result<(), Error> {
        future::poll_fn(|cx| Pin::new(&mut self).poll_close(cx)).await
    }
}

impl Stream for WebSocket {
    type Item = Result<Message, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        match ready!(Pin::new(&mut self.inner).poll_next(cx)) {
            Some(Ok(item)) => Poll::Ready(Some(Ok(Message { inner: item }))),
            Some(Err(e)) => {
                tracing::debug!("websocket poll error: {}", e);
                Poll::Ready(Some(Err(Error::new(e))))
            }
            None => {
                tracing::debug!("websocket closed");
                Poll::Ready(None)
            }
        }
    }
}

impl Sink<Message> for WebSocket {
    type Error = Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match ready!(Pin::new(&mut self.inner).poll_ready(cx)) {
            Ok(()) => Poll::Ready(Ok(())),
            Err(e) => Poll::Ready(Err(Error::new(e))),
        }
    }

    fn start_send(mut self: Pin<&mut Self>, item: Message) -> Result<(), Self::Error> {
        match Pin::new(&mut self.inner).start_send(item.inner) {
            Ok(()) => Ok(()),
            Err(e) => {
                tracing::debug!("websocket start_send error: {}", e);
                Err(Error::new(e))
            }
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        match ready!(Pin::new(&mut self.inner).poll_flush(cx)) {
            Ok(()) => Poll::Ready(Ok(())),
            Err(e) => Poll::Ready(Err(Error::new(e))),
        }
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        match ready!(Pin::new(&mut self.inner).poll_close(cx)) {
            Ok(()) => Poll::Ready(Ok(())),
            Err(err) => {
                tracing::debug!("websocket close error: {}", err);
                Poll::Ready(Err(Error::new(err)))
            }
        }
    }
}

impl fmt::Debug for WebSocket {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("WebSocket").finish()
    }
}

/// A WebSocket message.
///
/// This will likely become a `non-exhaustive` enum in the future, once that
/// language feature has stabilized.
#[derive(Eq, PartialEq, Clone)]
pub struct Message {
    inner: protocol::Message,
}

impl Message {
    /// Construct a new Text `Message`.
    pub fn text<S: Into<String>>(s: S) -> Message {
        Message {
            inner: protocol::Message::text(s),
        }
    }

    /// Construct a new Binary `Message`.
    pub fn binary<V: Into<Vec<u8>>>(v: V) -> Message {
        Message {
            inner: protocol::Message::binary(v),
        }
    }

    /// Construct a new Ping `Message`.
    pub fn ping<V: Into<Vec<u8>>>(v: V) -> Message {
        Message {
            inner: protocol::Message::Ping(v.into()),
        }
    }

    /// Construct the default Close `Message`.
    pub fn close() -> Message {
        Message {
            inner: protocol::Message::Close(None),
        }
    }

    /// Construct a Close `Message` with a code and reason.
    pub fn close_with(code: impl Into<u16>, reason: impl Into<Cow<'static, str>>) -> Message {
        Message {
            inner: protocol::Message::Close(Some(protocol::frame::CloseFrame {
                code: protocol::frame::coding::CloseCode::from(code.into()),
                reason: reason.into(),
            })),
        }
    }

    /// Returns true if this message is a Text message.
    pub fn is_text(&self) -> bool {
        self.inner.is_text()
    }

    /// Returns true if this message is a Binary message.
    pub fn is_binary(&self) -> bool {
        self.inner.is_binary()
    }

    /// Returns true if this message a is a Close message.
    pub fn is_close(&self) -> bool {
        self.inner.is_close()
    }

    /// Returns true if this message is a Ping message.
    pub fn is_ping(&self) -> bool {
        self.inner.is_ping()
    }

    /// Returns true if this message is a Pong message.
    pub fn is_pong(&self) -> bool {
        self.inner.is_pong()
    }

    /// Try to get the close frame (close code and reason)
    pub fn close_frame(&self) -> Option<(u16, &str)> {
        if let protocol::Message::Close(Some(ref close_frame)) = self.inner {
            Some((close_frame.code.into(), close_frame.reason.as_ref()))
        } else {
            None
        }
    }

    /// Try to get a reference to the string text, if this is a Text message.
    pub fn to_str(&self) -> Option<&str> {
        match self.inner {
            protocol::Message::Text(ref s) => Some(s),
            _ => None,
        }
    }

    /// Returns the bytes of this message, if the message can contain data.
    pub fn as_bytes(&self) -> &[u8] {
        match self.inner {
            protocol::Message::Text(ref s) => s.as_bytes(),
            protocol::Message::Binary(ref v) => v,
            protocol::Message::Ping(ref v) => v,
            protocol::Message::Pong(ref v) => v,
            protocol::Message::Close(_) => &[],
        }
    }

    /// Destructure this message into binary data.
    pub fn into_bytes(self) -> Vec<u8> {
        self.inner.into_data()
    }
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&self.inner, f)
    }
}

#[allow(clippy::from_over_into)]
impl Into<Vec<u8>> for Message {
    fn into(self) -> Vec<u8> {
        self.into_bytes()
    }
}

/// Connection header did not include 'upgrade'
#[derive(Debug)]
pub struct MissingConnectionUpgrade;

impl ::std::fmt::Display for MissingConnectionUpgrade {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "Connection header did not include 'upgrade'")
    }
}

impl ::std::error::Error for MissingConnectionUpgrade {}
