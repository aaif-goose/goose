use crate::errors::ProviderError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionRequest {
    pub model: String,
    pub state: Value,
    pub questions: HashMap<String, DecisionQuestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum DecisionQuestion {
    Noul {
        instructions: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        criteria: Option<NoulCriteria>,
    },
    Choice {
        instructions: String,
        criteria: HashMap<String, String>,
    },
    Score {
        instructions: String,
        criteria: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoulCriteria {
    #[serde(rename = "true")]
    pub true_description: String,
    #[serde(rename = "false")]
    pub false_description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionResponse {
    pub model: String,
    #[serde(default)]
    pub answers: HashMap<String, DecisionAnswer>,
    pub usage: DecisionUsage,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum DecisionAnswer {
    Noul {
        noul: f64,
    },
    Choice {
        choice: String,
        confidence: f64,
        probabilities: HashMap<String, f64>,
    },
    Score {
        score: f64,
        confidence: f64,
        legend: HashMap<String, Value>,
        probabilities: HashMap<String, f64>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionUsage {
    #[serde(default)]
    pub input_tokens: Option<u64>,
    #[serde(default)]
    pub output_tokens: Option<u64>,
    #[serde(default)]
    pub cost: Option<f64>,
}

#[async_trait]
pub trait DecisionProvider: Send + Sync {
    async fn create_decision(
        &self,
        request: &DecisionRequest,
    ) -> Result<DecisionResponse, ProviderError>;
}
