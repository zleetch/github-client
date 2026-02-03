use anyhow::{anyhow, Result};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct RepoResponse {
    pub full_name: String,
    pub html_url: String,
    pub default_branch: String,
}

#[derive(Serialize)]
struct GenerateRequest<'a> {
    name: &'a str,
    description: &'a str,
    private: bool,
    include_all_branches: bool,
}

pub async fn generate_from_template(
    api_base: &str,
    token: &str,
    template_name: &str,
    repo_name: &str,
    repo_desc: &str,
    is_private: bool,
    include_all_branches: bool,
) -> Result<RepoResponse> {
    let (template_owner, template_repo) = split_template_name(template_name)?;
    let url = format!(
        "{}/repos/{}/{}/generate",
        api_base.trim_end_matches('/'),
        template_owner,
        template_repo
    );

    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", token))?,
    );
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/vnd.github+json"),
    );
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("github-client-rust/0.1"),
    );
    headers.insert(
        HeaderName::from_static("x-github-api-version"),
        HeaderValue::from_static("2022-11-28"),
    );

    let client = reqwest::Client::builder()
        .default_headers(headers)
        .build()?;

    let body = GenerateRequest {
        name: repo_name,
        description: repo_desc,
        private: is_private,
        include_all_branches,
    };

    let resp = client.post(url).json(&body).send().await?;
    if resp.status().is_success() || resp.status().as_u16() == 201 {
        let repo: RepoResponse = resp.json().await?;
        return Ok(repo);
    }

    let status = resp.status();
    let text = resp
        .text()
        .await
        .unwrap_or_else(|_| "<no body>".to_string());
    Err(anyhow!(
        "GitHub API error (status {}): {}",
        status,
        text.trim()
    ))
}

fn split_template_name(template: &str) -> Result<(&str, &str)> {
    let mut parts = template.splitn(2, '/');
    let owner = parts
        .next()
        .ok_or_else(|| anyhow!("Invalid template name; expected 'owner/repo'"))?;
    let repo = parts
        .next()
        .ok_or_else(|| anyhow!("Invalid template name; expected 'owner/repo'"))?;
    if owner.is_empty() || repo.is_empty() {
        return Err(anyhow!(
            "Invalid template name; expected 'owner/repo', got '{}'",
            template
        ));
    }
    Ok((owner, repo))
}

#[cfg(test)]
mod tests {
    use super::split_template_name;

    #[test]
    fn split_template_name_ok() {
        let (o, r) = split_template_name("owner/repo").unwrap();
        assert_eq!(o, "owner");
        assert_eq!(r, "repo");
    }

    #[test]
    fn split_template_name_err() {
        assert!(split_template_name("invalid").is_err());
        assert!(split_template_name("owner/").is_err());
        assert!(split_template_name("/repo").is_err());
    }
}
