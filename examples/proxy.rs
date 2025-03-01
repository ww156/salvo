use salvo::prelude::*;
use salvo_extra::proxy::ProxyHandler;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new()
        .push(Router::with_path("google/<**rest>").handle(ProxyHandler::new(vec!["https://www.google.com".into()])))
        .push(Router::with_path("baidu/<**rest>").handle(ProxyHandler::new(vec!["https://www.baidu.com".into()])));
    tracing::info!("Listening on http://127.0.0.1:7878");
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}
