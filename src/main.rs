#[autumn_web::main]
async fn main() {
    autumn_web::app()
        .routes(autumn_io::app_routes())
        .static_routes(autumn_io::static_site_routes())
        .run()
        .await;
}
