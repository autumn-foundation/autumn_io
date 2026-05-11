#[autumn_web::main]
async fn main() {
    autumn_web::app()
        .routes(autumn_io::app_routes())
        .run()
        .await;
}
