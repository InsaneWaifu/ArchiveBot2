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
