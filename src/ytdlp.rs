use std::io::BufRead;

use poise::serenity_prelude::async_trait;

use crate::downloader::Downloader;

pub struct YoutubeDownloader {}

#[async_trait]
impl Downloader for YoutubeDownloader {
    async fn download(&self, url: String) -> Result<(String, tempfile::NamedTempFile), anyhow::Error> {
        let tempfile = tempfile::NamedTempFile::with_suffix(".mp4")?;
        let output = tokio::process::Command::new("yt-dlp")
            .arg("-o")
            .arg(tempfile.path())
            .arg("--recode-video")
            .arg("mp4")
            .arg("--force-overwrite")
            .arg("--no-playlist")
            .arg("-I")
            .arg("1:1")
            .arg("--max-downloads")
            .arg("4")
            .arg("--no-simulate")
            .arg("--progress")
            .arg("--print")
            .arg("VIDEOTITLE((![[%(title)s]]!))")
            .arg(&url)
            .output()
            .await?;
        anyhow::ensure!(output.status.success(), "youtube-dl failed with output {output:?}");
        // Look for a line with VIDEOTITLE(...)
        for line in output.stdout.lines() {
            let line = line?;
            if line.starts_with("VIDEOTITLE") {
                let title = line.split("((![[").nth(1).unwrap();
                let title = title.split("]]!").nth(0).unwrap();
                return Ok((title.to_owned(), tempfile));
            }
        }
        Ok((url, tempfile))
    }
}
