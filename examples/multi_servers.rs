use salvo::prelude::*;

#[fn_handler]
async fn hello_world1() -> &'static str {
    "Server1: Hello World"
}
#[fn_handler]
async fn hello_world2() -> &'static str {
    "Server2: Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router1 = Router::new().get(hello_world1);
    let router2 = Router::new().get(hello_world2);

    tracing::info!("Listening on http://127.0.0.1:7878");
    tracing::info!("Listening on http://127.0.0.1:7979");
    tokio::try_join!(
        Server::new(TcpListener::bind("127.0.0.1:7878")).try_serve(router1),
        Server::new(TcpListener::bind("127.0.0.1:7979")).try_serve(router2),
    )
    .unwrap();
}
