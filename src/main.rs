#[autumn_web::main]
async fn main() {
    autumn_web::app()
        .with_story_gallery(autumn_web::stories::StoryGallery::builtin())
        .routes(autumn_io::app_routes())
        .layer(autumn_io::response_compression_layer())
        // Project the JSON docs API (`src/api.rs`) into an MCP server, so any
        // coding agent can read the guides for the release that is deployed.
        // Deliberately unauthenticated: this is public documentation, and the
        // three tools it exposes are all reads.
        .mount_mcp(autumn_io::MCP_MOUNT_PATH)
        .run()
        .await;
}
