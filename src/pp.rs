use std::{
    any::TypeId,
    path::Path
};

use poise::serenity_prelude::async_trait;

use crate::{
    Data,
    db::{NewObject, Object, User},
};

#[async_trait]
pub trait PostProcessor {
    async fn check(&self, input: &PostProcessInput) -> bool;
    async fn process(&self, input: PostProcessInput) -> Result<PostProcessOutput, anyhow::Error>;
}

pub struct PostProcessInput {
    pub file: Object,
    pub user: User,
    pub previous_passes: Vec<TypeId>,
    pub data: Data,
}

pub struct PostProcessOutput {
    pub file: Object,
    pub additional_passes: Vec<TypeId>,
}

pub struct FFMpegResizeProcessor {
    pub max_size: u64,
}

#[async_trait]
impl PostProcessor for FFMpegResizeProcessor {
    async fn check(&self, input: &PostProcessInput) -> bool {
        std::fs::metadata(&input.file.path).unwrap().len() > self.max_size
            && ["mp4", "mkv", "webm", "avi", "mov", "gif"].contains(
                &Path::new(&input.file.path)
                    .extension()
                    .unwrap()
                    .to_str()
                    .unwrap(),
            )
    }

    async fn process(&self, input: PostProcessInput) -> Result<PostProcessOutput, anyhow::Error> {
        println!("FFMPEG pass processing {}", input.file.path);
        let prev_passes = input
            .previous_passes
            .iter()
            .filter(|x| **x == typeid::of::<Self>())
            .count();
        if prev_passes >= 3 {
            // Give up after 3 passes
            anyhow::bail!("Failed to shrink after 3 passes");
        }
        let new_max_size = self.max_size as f64 * 0.9;
        let new_max_size = new_max_size as f64 * (1. - (0.1 * prev_passes as f64));
        let new_max_size = new_max_size as u64;
        println!("new_max_size:{}", new_max_size);

        let mut target_audio_bitrate = None;
        let target_video_bitrate;
        // Ffprobe to get the current bitrates, and duration
        let ffprobe = std::process::Command::new("ffprobe")
            .arg("-v")
            .arg("quiet")
            .arg("-print_format")
            .arg("json")
            .arg("-show_entries")
            .arg("format=duration,bit_rate")
            .arg("-show_streams")
            .arg("-of")
            .arg("json")
            .arg(&input.file.path)
            .output()?;
        anyhow::ensure!(ffprobe.status.success(), "ffprobe failed");
        let ffprobe_output = std::str::from_utf8(&ffprobe.stdout)?;
        let ffprobe_output: serde_json::Value = serde_json::from_str(ffprobe_output)?;
        dbg!(&ffprobe_output);
        let duration = ffprobe_output
            .get("format")
            .unwrap()
            .get("duration")
            .unwrap()
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap() as u64;

        if let Some(streams) = ffprobe_output.get("streams") {
            for stream in streams.as_array().unwrap() {
                if stream.get("codec_type").unwrap().as_str().unwrap() == "audio" {
                    // Clamp audio bitrate to 128kbps
                    target_audio_bitrate = Some(std::cmp::min(
                        stream
                            .get("bit_rate")
                            .unwrap()
                            .as_str()
                            .unwrap()
                            .parse::<u64>()
                            .unwrap(),
                        128_000,
                    ));
                }
            }
            let audio_bitrate = target_audio_bitrate.unwrap_or(0);
            let video_bitrate = ((new_max_size * 8) / duration) - audio_bitrate;
            target_video_bitrate = Some(video_bitrate);
        } else {
            anyhow::bail!("No streams found in ffprobe output");
        }

        let mut object = NewObject::new_with_extension("mp4");

        // FFmpeg two pass encoding
        #[cfg(target_os = "linux")]
        const NULL_OUT: &str = "/dev/null";
        #[cfg(target_os = "windows")]
        const NULL_OUT: &str = "NUL";

        println!("Using audio bitrate {}", target_audio_bitrate.unwrap());
        println!("Using video bitrate {}", target_video_bitrate.unwrap());

        let status = tokio::process::Command::new("ffmpeg")
            .arg("-y")
            .arg("-nostdin")
            .arg("-i")
            .arg(&input.file.path) // Input file
            .arg("-preset")
            .arg("veryfast") // Preset
            .arg("-c:v") // Video codec
            .arg("libx264")
            .arg("-b:v")
            .arg(format!("{}", target_video_bitrate.unwrap()))
            .arg("-pass")
            .arg("1")
            .arg("-an") // Disable audio
            .arg("-f")
            .arg("null")
            .arg(NULL_OUT) // Output file
            .status()
            .await?;
        anyhow::ensure!(status.success(), "ffmpeg pass 1 failed");
        // Pass 2
        let status = tokio::process::Command::new("ffmpeg")
            .arg("-y")
            .arg("-nostdin")
            .arg("-i")
            .arg(&input.file.path) // Input file
            .arg("-preset")
            .arg("veryfast") // Preset
            .arg("-c:v") // Video codec
            .arg("libx264")
            .arg("-b:v")
            .arg(format!("{}", target_video_bitrate.unwrap()))
            .arg("-pass")
            .arg("2")
            .arg("-c:a") // Audio codec
            .arg("aac")
            .arg("-b:a") // Audio bitrate
            .arg(format!("{}", target_audio_bitrate.unwrap())) // Audio bitrate
            .arg(&object.path) // Output file
            .status()
            .await?;
        anyhow::ensure!(status.success(), "ffmpeg pass 2 failed");

        // Check the size of the output file
        let metadata = std::fs::metadata(&object.path)?;
        println!("size:{}", metadata.len());

        let newname = input.file.name + " (compressed)";
        object.name = newname;
        object.expiry_unix = input.file.expiry_unix;
        object.size = metadata.len() as i64;
        object.user = input.user.snowflake;

        let object = input
            .data
            .db
            .get()
            .await?
            .interact(move |x| {
                use diesel::prelude::*;

                diesel::insert_into(crate::schema::objects::table)
                    .values(&object)
                    .returning(Object::as_returning())
                    .get_result(x)
            })
            .await
            .unwrap()?;

        if metadata.len() > self.max_size {
            return Ok(PostProcessOutput {
                additional_passes: vec![typeid::of::<Self>()],
                file: object,
            });
        }
        Ok(PostProcessOutput {
            file: object,
            additional_passes: vec![],
        })
    }
}
