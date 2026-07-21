#[autumn_web::main]
async fn main() {
    autumn_web::app()
        .with_story_gallery(autumn_web::stories::StoryGallery::builtin())
        .routes(autumn_io::app_routes())
        .layer(autumn_io::response_compression_layer())
        .run()
        .await;
}
