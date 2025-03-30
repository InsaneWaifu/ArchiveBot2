use poise::serenity_prelude::async_trait;


#[async_trait]
pub trait Downloader {
    async fn download(&self, url: String) -> Result<tempfile::NamedTempFile, anyhow::Error>;
}
