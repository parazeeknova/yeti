use crate::config::Config;
use crate::error::{Result, YetiError};
use crate::prompt::SYSTEM_PROMPT;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::time::Duration;

const API_URL: &str = "https://api.cerebras.ai/v1/chat/completions";
const REQUEST_TIMEOUT_SECS: u64 = 60;

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_completion_tokens: Option<u32>,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct StreamResponse {
    choices: Vec<StreamChoice>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: Delta,
}

#[derive(Debug, Deserialize)]
struct Delta {
    #[serde(default)]
    content: Option<String>,
}

pub fn generate_commit_message(
    api_key: &str,
    model: &str,
    user_prompt: &str,
    on_chunk: impl Fn(&str),
) -> Result<String> {
    let request = ChatRequest {
        model: model.to_string(),
        messages: vec![
            Message {
                role: "system".to_string(),
                content: SYSTEM_PROMPT.to_string(),
            },
            Message {
                role: "user".to_string(),
                content: user_prompt.to_string(),
            },
        ],
        temperature: Some(0.0),
        max_completion_tokens: Some(500),
        stream: true,
    };

    let body = serde_json::to_string(&request)?;

    let agent = http_agent();
    let response = agent
        .post(API_URL)
        .header("Authorization", &format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .send(&body)
        .map_err(handle_ureq_error)?;

    let status = response.status();
    if !status.is_success() {
        let status_code = status.as_u16();
        let body_text = response.into_body().read_to_string().unwrap_or_default();
        return Err(YetiError::ApiError {
            status: status_code,
            message: body_text,
        });
    }

    let mut full_content = String::new();
    let reader = BufReader::new(response.into_body().into_reader());

    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(e) => return Err(YetiError::NetworkError(e.to_string())),
        };

        if line.is_empty() || !line.starts_with("data: ") {
            continue;
        }

        let data = &line[6..];
        if data == "[DONE]" {
            break;
        }

        let stream_resp: StreamResponse = match serde_json::from_str(data) {
            Ok(r) => r,
            Err(_) => continue,
        };

        if let Some(choice) = stream_resp.choices.first()
            && let Some(content) = &choice.delta.content
        {
            on_chunk(content);
            full_content.push_str(content);
        }
    }

    Ok(full_content)
}

pub fn validate_api_key(api_key: &str) -> Result<bool> {
    let request = ChatRequest {
        model: Config::default_model().to_string(),
        messages: vec![Message {
            role: "user".to_string(),
            content: "Hi".to_string(),
        }],
        temperature: None,
        max_completion_tokens: Some(10),
        stream: false,
    };

    let body = serde_json::to_string(&request)?;

    let agent = http_agent();
    let response = agent
        .post(API_URL)
        .header("Authorization", &format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .send(&body);

    match response {
        Ok(resp) if resp.status().is_success() => Ok(true),
        Ok(resp) if resp.status().as_u16() == 401 => {
            Err(YetiError::InvalidApiKey("Invalid API key".to_string()))
        }
        Ok(resp) => Err(YetiError::ApiError {
            status: resp.status().as_u16(),
            message: "API request failed".to_string(),
        }),
        Err(e) => Err(handle_ureq_error(e)),
    }
}

pub fn check_provider_ready(api_key: &str, model: &str) -> Result<()> {
    let request = ChatRequest {
        model: model.to_string(),
        messages: vec![Message {
            role: "user".to_string(),
            content: "ping".to_string(),
        }],
        temperature: None,
        max_completion_tokens: Some(4),
        stream: false,
    };

    let body = serde_json::to_string(&request)?;
    let agent = http_agent();
    let response = agent
        .post(API_URL)
        .header("Authorization", &format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .send(&body)
        .map_err(handle_ureq_error)?;

    if !response.status().is_success() {
        return Err(YetiError::ApiError {
            status: response.status().as_u16(),
            message: "Provider readiness check failed".to_string(),
        });
    }

    Ok(())
}

fn http_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(REQUEST_TIMEOUT_SECS)))
        .timeout_per_call(Some(Duration::from_secs(REQUEST_TIMEOUT_SECS)))
        .build()
        .new_agent()
}

fn handle_ureq_error(e: ureq::Error) -> YetiError {
    let err_str = e.to_string();
    if err_str.contains("401") {
        YetiError::InvalidApiKey("Authentication failed".to_string())
    } else if err_str.contains("429") {
        YetiError::ApiError {
            status: 429,
            message: "Rate limited. Please wait and try again.".to_string(),
        }
    } else {
        YetiError::NetworkError(err_str)
    }
}

fn sanitize_message(raw: &str) -> (String, Option<String>) {
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .collect();

    let lines: Vec<&str> = cleaned
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .filter(|l| !l.starts_with('#'))
        .filter(|l| !l.starts_with("```"))
        .collect();

    if lines.is_empty() {
        return ("chore: update files".to_string(), None);
    }

    let title = lines[0].to_string();
    let title = title.chars().take(72).collect();

    let body_lines: Vec<&str> = lines
        .iter()
        .skip(1)
        .filter(|l| l.len() > 10)
        .take(3)
        .cloned()
        .collect();

    let body = if body_lines.is_empty() {
        None
    } else {
        Some(body_lines.join("\n"))
    };

    (title, body)
}

pub fn parse_commit_message(raw: &str) -> (String, Option<String>) {
    sanitize_message(raw)
}

#[cfg(test)]
mod tests {
    use super::parse_commit_message;

    #[test]
    fn parse_commit_message_strips_markdown_and_builds_body() {
        let raw = "```text\n# heading\nfix[CORE]: handle rename metadata\n\nAdd rename source path to prompt context.\n```";
        let (title, body) = parse_commit_message(raw);

        assert_eq!(title, "fix[CORE]: handle rename metadata");
        assert_eq!(
            body.as_deref(),
            Some("Add rename source path to prompt context.")
        );
    }

    #[test]
    fn parse_commit_message_falls_back_when_content_is_empty() {
        let raw = "\u{0000}\u{0007}\n```";
        let (title, body) = parse_commit_message(raw);

        assert_eq!(title, "chore: update files");
        assert!(body.is_none());
    }

    #[test]
    fn parse_commit_message_limits_title_and_body_lines() {
        let long_title = format!("feat[CORE]: {}", "x".repeat(120));
        let raw = format!(
            "{long_title}\n\nshort\nBody line one is long enough.\nBody line two is long enough.\nBody line three is long enough.\nBody line four is long enough."
        );

        let (title, body) = parse_commit_message(&raw);

        assert_eq!(title.chars().count(), 72);
        assert_eq!(
            body.as_deref(),
            Some(
                "Body line one is long enough.\nBody line two is long enough.\nBody line three is long enough."
            )
        );
    }
}
