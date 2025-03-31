use std::{
    any::TypeId,
    collections::{HashMap, VecDeque},
    time::{Duration, SystemTime},
};

use anyhow::Error;
use db::{NewObject, Object, SharexConfig, User};
use diesel::query_dsl::methods::FilterDsl;
use downloader::Downloader;
use poise::{
    CreateReply,
    serenity_prelude::{
        self as serenity, ComponentInteractionDataKind, CreateActionRow, CreateAttachment,
        CreateButton, CreateEmbed, CreateInteractionResponseFollowup, CreateSelectMenu,
        CreateSelectMenuOption, Embed, Interaction, MessageCommandInteractionMetadata,
        MessageInteractionMetadata,
    },
};
use pp::{FFMpegResizeProcessor, PostProcessInput, PostProcessor};
use ytdlp::YoutubeDownloader;
mod db;
mod downloader;
mod gallerydl;
mod pp;
mod schema;
mod sharex;
mod ytdlp;

#[derive(Clone)]
pub struct Data {
    db: db::DatabasePool,
} // User data, which is stored and accessible in all command invocations

type Context<'a> = poise::Context<'a, Data, Error>;

pub struct PostProcessOrchestrator<'a> {
    user: poise::serenity_prelude::User,
    object: Object,
    data: Data,
    post_processors: HashMap<TypeId, &'a (dyn PostProcessor + Sync + Send)>,
    postprocessors_to_run: Vec<TypeId>,
}

impl<'a> PostProcessOrchestrator<'a> {
    pub fn new(user: poise::serenity_prelude::User, object: Object, data: Data) -> Self {
        Self {
            user,
            object,
            data,
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

    pub async fn process(&mut self) -> Result<(), anyhow::Error> {
        let uid = self.user.id.get() as i64;
        let username = self.user.name.clone();
        let user = self
            .data
            .db
            .get()
            .await?
            .interact(move |x| User::get_or_create(uid, username, x))
            .await
            .unwrap()?;
        let mut input = PostProcessInput {
            file: self.object.clone(),
            previous_passes: vec![],
            data: self.data.clone(),
            user,
        };
        let mut queue = self
            .postprocessors_to_run
            .iter()
            .cloned()
            .collect::<VecDeque<_>>();
        while let Some(typeid) = queue.pop_front() {
            println!("Postprocessors to run: {:?}", queue);
            let post_processor = self.post_processors.get(&typeid).unwrap();
            let mut previous_passes = input.previous_passes.clone();
            let user = input.user.clone();
            let data = input.data.clone();
            if post_processor.check(&input).await {
                let output = post_processor.process(input).await?;
                previous_passes.push(typeid);
                input = PostProcessInput {
                    file: output.file,
                    previous_passes,
                    user,
                    data,
                };
                queue.extend(output.additional_passes);
            }
        }
        self.object = input.file;
        Ok(())
    }
}

fn embed_object(object: Object) -> Result<CreateReply, Error> {
    let object_expiry_time =
        SystemTime::UNIX_EPOCH + Duration::from_secs(object.expiry_unix as u64);
    let days_until_expiry = object_expiry_time
        .duration_since(SystemTime::now())
        .unwrap()
        .as_secs()
        / (60 * 60 * 24);
    let embed = CreateEmbed::new()
        .title(object.name)
        .description(humansize::format_size(
            object.size as u64,
            humansize::DECIMAL,
        ))
        .color(serenity::Color::from_rgb(0, 0, 255))
        .field("Expires", format!("In {days_until_expiry} days"), false);
    let dropdown = CreateSelectMenu::new(
        format!("Object:{}", object.id),
        serenity::CreateSelectMenuKind::String {
            options: vec![
                CreateSelectMenuOption::new("Upload to discord", "upload"),
                CreateSelectMenuOption::new("Delete", "delete"),
                CreateSelectMenuOption::new("Compress to discord size (10mb)", "compress"),
                CreateSelectMenuOption::new("Compress to discord size (50mb nitro)", "compress50"),
                CreateSelectMenuOption::new("Upload to XBackbone", "xbackbone"),
                CreateSelectMenuOption::new("--", "--"),
            ],
        },
    );
    let action_row = CreateActionRow::SelectMenu(dropdown);
    Ok(CreateReply {
        reply: true,
        components: Some(vec![action_row]),
        embeds: vec![embed],
        ..Default::default()
    })
}

#[poise::command(slash_command, install_context = "Guild|User", interaction_context = "Guild|BotDm|PrivateChannel")]
async fn ytdlp(ctx: Context<'_>, #[description = "Video URL"] url: String) -> Result<(), Error> {
    ctx.defer().await?;
    let downloader = YoutubeDownloader {};
    let (name, tmp) = downloader.download(url).await?;
    let path = tmp.into_temp_path().keep()?;
    let metadata = std::fs::metadata(&path)?;
    let size = metadata.len() as i64;
    let expiry_time = SystemTime::now() + Duration::from_secs(60 * 60 * 24 * 7);
    let expiry_unix = expiry_time
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs() as i64;
    let object = NewObject {
        path: path.to_string_lossy().to_string(),
        name,
        size,
        expiry_unix,
        user: ctx.author().id.get() as i64,
    };
    let object = ctx
        .data()
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
    let respond = embed_object(object)?;
    ctx.send(respond).await?;
    Ok(())
}

#[poise::command(slash_command, install_context = "Guild|User", interaction_context = "Guild|BotDm|PrivateChannel")]
async fn my_objects(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let user_id = ctx.author().id.get() as i64;
    let objects = ctx
        .data()
        .db
        .get()
        .await?
        .interact(move |x| {
            use crate::schema::objects::dsl::*;
            use diesel::prelude::*;
            diesel::QueryDsl::filter(objects, user.eq(user_id))
                .select(Object::as_select())
                .order_by(expiry_unix.desc())
                .load(x)
        })
        .await
        .unwrap()?;
    let mut str = "You own the following objects: ".to_owned();
    for object in objects {
        str.push_str(&format!("([{}]-{})\n", object.id, object.name));
    }
    ctx.reply(str).await?;
    Ok(())
}

#[poise::command(slash_command, install_context = "Guild|User", interaction_context = "Guild|BotDm|PrivateChannel")]
async fn get_object(ctx: Context<'_>, #[description = "Object ID"] oid: i32) -> Result<(), Error> {
    ctx.defer().await?;
    let uid = ctx.author().id.get() as i64;
    let object = ctx
        .data()
        .db
        .get()
        .await?
        .interact(move |x| {
            use crate::schema::objects::dsl::*;
            use diesel::prelude::*;
            diesel::QueryDsl::filter(objects.find(oid), user.eq(uid))
                .select(Object::as_select())
                .first(x)
        })
        .await
        .unwrap()?;
    let create_reply = embed_object(object)?;
    ctx.send(create_reply).await?;
    Ok(())
}

#[poise::command(slash_command, install_context = "Guild|User", interaction_context = "BotDm")]
async fn upload_xbackbone_config(ctx: Context<'_>, json_text: String) -> Result<(), Error> {
    ctx.defer().await?;
    let uid = ctx.author().id.get() as i64;
    ctx.data()
        .db
        .get()
        .await?
        .interact(move |x| {
            use diesel::prelude::*;
            diesel::insert_into(crate::schema::sharex_config::table)
                .values(
                    SharexConfig {
                        user_id: uid,
                        json: json_text.clone(),
                    }
                )
                .execute(x)
        })
        .await
        .unwrap()?;
    ctx.reply("Received sharex config").await?;
    Ok(())
}

async fn event_handler(
    ctx: &serenity::Context,
    event: &serenity::FullEvent,
    _framework: poise::FrameworkContext<'_, Data, Error>,
    data: &Data,
) -> Result<(), Error> {
    match event {
        serenity::FullEvent::InteractionCreate { interaction } => {
            match interaction {
                Interaction::Component(component) => {
                    let object_id = component.data.custom_id.strip_prefix("Object:");
                    if object_id.is_none() {
                        return Ok(());
                    };
                    let object_id: i32 = object_id.unwrap().parse()?;
                    let object = data
                        .db
                        .get()
                        .await?
                        .interact(move |x| {
                            use crate::schema::objects::dsl::*;
                            use diesel::prelude::*;
                            objects.find(object_id).select(Object::as_select()).first(x)
                        })
                        .await
                        .unwrap()?;
                    if object.user != component.user.id.get() as i64 {
                        return Ok(());
                    }
                    let chosen_action = match &component.data.kind {
                        ComponentInteractionDataKind::StringSelect { values } => {
                            values.get(0).unwrap()
                        }
                        _ => anyhow::bail!("Invalid component type"),
                    };
                    match chosen_action.as_str() {
                        "delete" => {
                            std::fs::remove_file(&object.path)?;
                            data.db
                                .get()
                                .await?
                                .interact(move |x| {
                                    use crate::schema::objects::dsl::*;
                                    use diesel::prelude::*;
                                    diesel::delete(objects.find(object_id)).execute(x)
                                })
                                .await
                                .unwrap()?;
                            component
                                .create_response(
                                    &ctx,
                                    serenity::CreateInteractionResponse::Acknowledge,
                                )
                                .await?;
                        }
                        "compress" | "compress50" => {
                            let compress = async |max_size: u64| {
                                // Run ffmpeg pass on the file
                                println!("Compressing to {}, defer", max_size);
                                component.defer(&ctx).await?;
                                let mut pp_orchestrator = PostProcessOrchestrator::new(
                                    component.user.clone(),
                                    object.clone(),
                                    data.clone(),
                                );
                                let ffmpeg = FFMpegResizeProcessor { max_size };
                                pp_orchestrator.add_post_processor(&ffmpeg, true);
                                println!("Running compress pass");
                                pp_orchestrator.process().await?;
                                let new_object = pp_orchestrator.object;
                                let embed = embed_object(new_object)?;
                                component
                                    .create_followup(
                                        &ctx,
                                        CreateInteractionResponseFollowup::new()
                                            .embeds(embed.embeds)
                                            .components(embed.components.unwrap()),
                                    )
                                    .await?;
                                Ok::<(), anyhow::Error>(())
                            };
                            match chosen_action.as_str() {
                                "compress" => compress(9_500_000).await?,
                                "compress50" => compress(49_000_000).await?,
                                _ => anyhow::bail!("Invalid component type"),
                            }
                            println!("Finished compress pass");
                        }
                        "upload" => {
                            component.defer(&ctx).await?;
                            component
                                .create_followup(
                                    &ctx,
                                    CreateInteractionResponseFollowup::new()
                                        .add_file(CreateAttachment::path(object.path).await?),
                                )
                                .await?;
                        }
                        "xbackbone" => {
                            component.defer(&ctx).await?;
                            let uid = component.user.id.get() as i64;
                            let sharex_json = data
                                .db
                                .get()
                                .await?
                                .interact(move |x| {
                                    use crate::schema::sharex_config::dsl::*;
                                    use diesel::prelude::*;
                                    sharex_config
                                        .find(uid)
                                        .select(SharexConfig::as_select())
                                        .first(x)
                                })
                                .await
                                .unwrap()?;
                            let sharex_json: serde_json::Result<sharex::XBackboneShareXData> =
                                serde_json::from_str(&sharex_json.json);
                            if let Err(e) = sharex_json {
                                println!("Error parsing sharex json: {}", e);
                                component
                                    .create_followup(
                                        &ctx,
                                        CreateInteractionResponseFollowup::new().add_embed(
                                            CreateEmbed::new()
                                                .title("Error parsing sharex json")
                                                .description(format!("{}", e))
                                                .color(serenity::Color::from_rgb(255, 0, 0)),
                                        ),
                                    )
                                    .await?;
                                return Ok(());
                            }
                            let sharex_json = sharex_json.unwrap();
                            println!("Uploading to xbackbone");
                            // Post a reqwest to upload
                            let client = reqwest::Client::new();
                            let form = reqwest::multipart::Form::new()
                                .text("token", sharex_json.arguments.token)
                                .file("file", &object.path)
                                .await?;
                            let response = client
                                .post(&sharex_json.request_url)
                                .multipart(form)
                                .send()
                                .await?;
                            let returned_data: sharex::ReturnedData = response.json().await?;
                            println!("Uploaded to {}", returned_data.url);
                            component
                                .create_followup(
                                    &ctx,
                                    CreateInteractionResponseFollowup::new()
                                        .content(returned_data.url),
                                )
                                .await?;
                        }
                        _ => println!("Unrecognized action {}", chosen_action.as_str()),
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().unwrap();
    tracing_subscriber::fmt::init();
    let token = std::env::var("DISCORD_TOKEN").expect("missing DISCORD_TOKEN");
    let intents = serenity::GatewayIntents::non_privileged();

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![ytdlp(), my_objects(), get_object(), upload_xbackbone_config()],
            event_handler: |a, b, c, d| Box::pin(event_handler(a, b, c, d)),
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(
                    ctx,
                    &framework.options().commands,
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
