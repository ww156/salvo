use salvo::prelude::*;
use salvo::Catcher;

#[fn_handler]
async fn hello_world() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(hello_world);
    let catcher: Vec<Box<dyn Catcher>> = vec![Box::new(Handle404)];
    let service = Service::new(router).with_catchers(catcher);
    tracing::info!("Listening on http://127.0.0.1:7878");
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(service).await;
}

struct Handle404;
impl Catcher for Handle404 {
    fn catch(&self, _req: &Request, _depot: &Depot, res: &mut Response) -> bool {
        if let Some(StatusCode::NOT_FOUND) = res.status_code() {
            res.render_plain_text("Custom 404 Error Page");
            true
        } else {
            false
        }
    }
}
