#[autumn_web::main]
async fn main() {
    autumn_web::app()
        .routes(autumn_io::app_routes())
        .layer(autumn_io::response_compression_layer())
        .run()
        .await;
}
