use poise::{Context, CreateReply};
use poise::serenity_prelude::{async_trait, CreateAttachment};
use tempfile::NamedTempFile;

#[async_trait]
pub trait Terminator {
    async fn finish(&self, input: NamedTempFile) -> Result<(), anyhow::Error>;
}

pub struct DiscordTerminator<'a>(pub Context<'a, crate::Data, anyhow::Error>);

#[async_trait]
impl Terminator for DiscordTerminator<'_> {
    async fn finish(&self, input: NamedTempFile) -> Result<(), anyhow::Error> {
        self.0.send(CreateReply {
            content: Some("Finished".to_string()),
            attachments: vec![CreateAttachment::path(input.path()).await?],
            ..Default::default()
        }).await?;
        Ok(())
    }
}




use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XBackboneShareXData {
    #[serde(rename = "RequestURL")]
    pub request_url: String,
    #[serde(rename = "FileFormName")]
    pub file_form_name: String,
    #[serde(rename = "Arguments")]
    pub arguments: Arguments,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Arguments {
    pub token: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReturnedData {
    #[serde(rename="url")]
    pub url: String,
}


pub struct XBackboneTerminator<'a>(Context<'a, crate::Data, anyhow::Error>, XBackboneShareXData);

#[async_trait]
impl Terminator for XBackboneTerminator<'_> {
    async fn finish(&self, input: NamedTempFile) -> Result<(), anyhow::Error> {
        // Upload the file to XBackbone
        let client = reqwest::Client::new();
        let form = reqwest::multipart::Form::new()
            .text("token", self.1.arguments.token.clone())
            .file("file", input.path()).await?;
        let response = client.post(self.1.request_url.clone())
            .multipart(form)
            .send().await?;
        let returned_data: ReturnedData = response.json().await?;
        self.0.send(CreateReply {
            content: Some(returned_data.url),
            ..Default::default()
        }).await?;
        Ok(())
    }
}
