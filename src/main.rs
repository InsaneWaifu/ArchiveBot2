use std::{
    any::TypeId,
    collections::{HashMap, VecDeque},
};

use anyhow::Error;
use downloader::Downloader;
use poise::serenity_prelude as serenity;
use pp::{FFMpegResizeProcessor, PostProcessInput, PostProcessor};
use terminator::{DiscordTerminator, Terminator};
use ytdlp::YoutubeDownloader;
mod db;
mod downloader;
mod ffmpeg;
mod gallerydl;
mod pp;
mod terminator;
mod ytdlp;

pub struct Data {
    db: db::DatabasePool,
} // User data, which is stored and accessible in all command invocations

type Context<'a> = poise::Context<'a, Data, Error>;

pub struct DownloadOrchestrator<'a> {
    ctx: Context<'a>,
    url: String,
    tempfile: Option<tempfile::NamedTempFile>,
    post_processors: HashMap<TypeId, &'a (dyn PostProcessor + Sync + Send)>,
    postprocessors_to_run: Vec<TypeId>,
}

impl<'a> DownloadOrchestrator<'a> {
    pub fn new(ctx: Context<'a>, url: String) -> Self {
        Self {
            ctx,
            url,
            tempfile: None,
            post_processors: HashMap::new(),
            postprocessors_to_run: vec![],
        }
    }

    pub fn add_post_processor<T: PostProcessor + Sync + Send>(
        &mut self,
        post_processor: &'a T,
        and_run: bool,
    ) -> &mut Self {
        self.post_processors
            .insert(typeid::of::<T>(), post_processor);
        if and_run {
            self.postprocessors_to_run.push(typeid::of::<T>());
        }
        self
    }

    pub async fn download_with<T: Downloader>(
        &mut self,
        downloader: T,
    ) -> Result<(), anyhow::Error> {
        let tempfile = downloader.download(self.url.clone()).await?;
        self.tempfile = Some(tempfile);
        Ok(())
    }

    pub async fn process(&mut self) -> Result<(), anyhow::Error> {
        let mut input = PostProcessInput {
            file: self.tempfile.take().unwrap(),
            previous_passes: vec![],
        };
        let mut queue = self
            .postprocessors_to_run
            .iter()
            .cloned()
            .collect::<VecDeque<_>>();
        while let Some(typeid) = queue.pop_front() {
            let post_processor = self.post_processors.get(&typeid).unwrap();
            let mut previous_passes = input.previous_passes.clone();
            if post_processor.check(&input).await {
                let output = post_processor.process(input).await?;
                previous_passes.push(typeid);
                input = PostProcessInput {
                    file: output.file,
                    previous_passes,
                };
                queue.extend(output.additional_passes);
            }
        }
        self.tempfile = Some(input.file);
        Ok(())
    }

    pub async fn use_uploader<T: Terminator>(&mut self, uploader: T) -> Result<(), anyhow::Error> {
        let tempfile = self.tempfile.take().unwrap();
        uploader.finish(tempfile).await?;
        Ok(())
    }
}

#[poise::command(slash_command)]
async fn ytdlp(ctx: Context<'_>, #[description = "Video URL"] url: String) -> Result<(), Error> {
    ctx.defer().await?;
    let mut orchestrator: DownloadOrchestrator<'_> = DownloadOrchestrator::new(ctx, url);
    let ffmpeg = FFMpegResizeProcessor {
        max_size: (1000.*1000.*9.5) as u64, //  10 MB max_size
    };
    orchestrator.add_post_processor(&ffmpeg, true);
    orchestrator.download_with(YoutubeDownloader {}).await?;
    orchestrator.process().await?;
    orchestrator.use_uploader(DiscordTerminator(ctx)).await?;
    Ok(())
}

#[tokio::main]
async fn main() {

    dotenvy::dotenv().unwrap();
    let token = std::env::var("DISCORD_TOKEN").expect("missing DISCORD_TOKEN");
    let intents = serenity::GatewayIntents::non_privileged();

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![ytdlp()],
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_in_guild(
                    ctx,
                    &framework.options().commands,
                    1145090805065855098.into(),
                )
                .await?;
                Ok(Data {
                    db: db::create_database_pool().await,
                })
            })
        })
        .build();

    let client = serenity::ClientBuilder::new(token, intents)
        .framework(framework)
        .await;
    client.unwrap().start().await.unwrap();
}
