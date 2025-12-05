pub mod routes;

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "static/"]
pub struct StaticAssets;
