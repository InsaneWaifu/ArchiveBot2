use poise::serenity_prelude::async_trait;
use tokio::io::AsyncBufReadExt;

use crate::downloader::Downloader;

pub struct YoutubeDownloader {}

#[async_trait]
impl Downloader for YoutubeDownloader {
    async fn download(&self, url: String) -> Result<(String, tempfile::NamedTempFile), anyhow::Error> {
        let tempfile = tempfile::NamedTempFile::with_suffix(".mp4")?;
        let mut command = tokio::process::Command::new("yt-dlp");
        command
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
            .arg("VIDEOTITLE((![[%(title)s]]!))");
        if std::env::var("YTDLP_COOKIES_FILE").is_ok() {
            command.arg("--cookies").arg(std::env::var("YTDLP_COOKIES_FILE").unwrap());
        };
        command
            .arg("--no-quiet")
            .arg(&url);
        // We want to capture stdout but also log it with tracing-appender
        command.stdout(std::process::Stdio::piped());
        let mut child = command.spawn()?;
        
        
        let stdout = child.stdout.take().unwrap();
        let bufreader = tokio::io::BufReader::new(stdout);
        let mut lines = bufreader.lines();
        let mut vidtitle = None;
        while let Ok(Some(line)) = lines.next_line().await {
            tracing::info!("{}", line);
            if line.starts_with("VIDEOTITLE") {
                let title = line.split("((![[").nth(1).unwrap();
                let title = title.split("]]!))").nth(0).unwrap();
                vidtitle = Some(title.to_owned());
            }
        }
        let status = child.wait().await?; 
        anyhow::ensure!(status.success(), "youtube-dl failed");

        Ok((vidtitle.unwrap_or(url), tempfile))
    }
}
