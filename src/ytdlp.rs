use poise::serenity_prelude::async_trait;

use crate::downloader::Downloader;


pub struct YoutubeDownloader {}

#[async_trait]
impl Downloader for YoutubeDownloader {
    async fn download(&self, url: String) -> Result<tempfile::NamedTempFile, anyhow::Error> {
        let tempfile = tempfile::NamedTempFile::with_suffix(".mp4")?;
        let status = std::process::Command::new("yt-dlp")
            .arg("-o")
            .arg(tempfile.path())
            .arg("--recode-video")
            .arg("mp4")
            .arg("--force-overwrite")
            .arg(url)
            .status()?;
        anyhow::ensure!(status.success(), "youtube-dl failed");
        Ok(tempfile)
    }
}


