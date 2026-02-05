use anyhow::{anyhow, Result};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::{Deserialize, Serialize};
use tokio::time::{sleep, Duration};
use tracing::{debug, info, warn};

#[derive(Deserialize)]
pub struct RepoResponse {
    pub full_name: String,
    pub html_url: String,
    pub default_branch: String,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct ApiErrorDetail {
    resource: Option<String>,
    field: Option<String>,
    code: Option<String>,
    message: Option<String>,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct ApiError {
    message: String,
    errors: Option<Vec<ApiErrorDetail>>,
    documentation_url: Option<String>,
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

    info!(
        "Generating repository '{}' from template '{}/{}'",
        repo_name, template_owner, template_repo
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

    debug!(
        "POST to GitHub API: include_all_branches={}, private={}",
        include_all_branches, is_private
    );
    let resp = client.post(url).json(&body).send().await?;
    let status = resp.status();
    if status.is_success() || status.as_u16() == 201 {
        let repo: RepoResponse = resp.json().await?;
        info!("Successfully created repository '{}'", repo.full_name);
        return Ok(repo);
    }

    // Try to decode structured error if possible
    let content_type_is_json = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("application/json"))
        .unwrap_or(false);

    if content_type_is_json {
        let api_err: ApiError = resp.json().await.unwrap_or(ApiError {
            message: "Unknown error".to_string(),
            errors: None,
            documentation_url: None,
        });
        warn!("GitHub API error {}: {}", status, api_err.message);

        let friendly = match status.as_u16() {
            403 => "Forbidden: token lacks required permissions. Ensure fine-grained PAT has Administration: Read & write on your account and Contents: Read on the template (or use classic PAT with repo/public_repo).".to_string(),
            404 => "Not found: template is not accessible or does not exist. Verify 'owner/repo' and that the repository is marked as a Template.".to_string(),
            422 => {
                if let Some(errors) = &api_err.errors {
                    if errors.iter().any(|e| {
                        e.code
                            .as_deref()
                            .map(|c| c.eq_ignore_ascii_case("already_exists"))
                            .unwrap_or(false)
                            || e.message
                                .as_deref()
                                .map(|m| m.to_lowercase().contains("already exists"))
                                .unwrap_or(false)
                    }) {
                        "Validation failed: a repository with this name already exists. Choose a different repo_name.".to_string()
                    } else {
                        "Validation failed: check repo_name and inputs.".to_string()
                    }
                } else {
                    "Validation failed: check repo_name and inputs.".to_string()
                }
            }
            _ => format!("GitHub API error {}: {}", status, api_err.message),
        };

        let details = api_err
            .errors
            .as_ref()
            .map(|v| format!(" details={:?}", v))
            .unwrap_or_default();
        return Err(anyhow!("{}{}", friendly, details));
    }

    // Fallback to plain text
    let text = resp
        .text()
        .await
        .unwrap_or_else(|_| "<no body>".to_string());
    warn!("GitHub API error {}: {}", status, text.trim());
    Err(anyhow!(format!(
        "GitHub API error (status {}): {}",
        status,
        text.trim()
    )))
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

#[derive(Serialize)]
struct RequiredStatusChecks<'a> {
    strict: bool,
    contexts: &'a [String],
}

#[derive(Serialize)]
struct RequiredPullRequestReviews {
    required_approving_review_count: u8,
    dismiss_stale_reviews: bool,
    require_code_owner_reviews: bool,
    require_last_push_approval: bool,
}

#[derive(Serialize)]
struct BranchProtectionRequest<'a> {
    required_status_checks: RequiredStatusChecks<'a>,
    enforce_admins: bool,
    required_pull_request_reviews: RequiredPullRequestReviews,
    restrictions: Option<serde_json::Value>,
    allow_force_pushes: bool,
    allow_deletions: bool,
    required_linear_history: bool,
    block_creations: bool,
    required_conversation_resolution: bool,
    lock_branch: bool,
    allow_fork_syncing: bool,
}

pub async fn protect_branch(
    api_base: &str,
    token: &str,
    full_name: &str,
    branch: &str,
) -> Result<()> {
    let (owner, repo) = split_template_name(full_name)?;
    let url = format!(
        "{}/repos/{}/{}/branches/{}/protection",
        api_base.trim_end_matches('/'),
        owner,
        repo,
        branch
    );

    info!(
        "Applying branch protection to '{}/{}' (branch '{}')",
        owner, repo, branch
    );

    // Wait for the branch to exist (new repos can be slightly delayed)
    ensure_branch_exists(api_base, token, full_name, branch, Duration::from_secs(30)).await?;

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

    let contexts: Vec<String> = Vec::new();
    let body = BranchProtectionRequest {
        required_status_checks: RequiredStatusChecks {
            strict: true,
            contexts: &contexts,
        },
        enforce_admins: true,
        required_pull_request_reviews: RequiredPullRequestReviews {
            required_approving_review_count: 1,
            dismiss_stale_reviews: true,
            require_code_owner_reviews: false,
            require_last_push_approval: true,
        },
        restrictions: None,
        allow_force_pushes: false,
        allow_deletions: false,
        required_linear_history: true,
        block_creations: false,
        required_conversation_resolution: true,
        lock_branch: false,
        allow_fork_syncing: false,
    };

    debug!("PUT branch protection payload prepared");
    let resp = client.put(url).json(&body).send().await?;
    let status = resp.status();
    if status.is_success() {
        info!("Branch protection applied");
        return Ok(());
    }

    let text = resp
        .text()
        .await
        .unwrap_or_else(|_| "<no body>".to_string());
    warn!(
        "Failed to apply branch protection {}: {}",
        status,
        text.trim()
    );
    Err(anyhow!(format!(
        "Failed to apply branch protection (status {}): {}",
        status,
        text.trim()
    )))
}

pub async fn protect_branch_with_checks(
    api_base: &str,
    token: &str,
    full_name: &str,
    branch: &str,
    required_contexts: &[&str],
) -> Result<()> {
    let (owner, repo) = split_template_name(full_name)?;
    let url = format!(
        "{}/repos/{}/{}/branches/{}/protection",
        api_base.trim_end_matches('/'),
        owner,
        repo,
        branch
    );

    info!(
        "Applying branch protection (with checks) to '{}/{}' (branch '{}'): {:?}",
        owner, repo, branch, required_contexts
    );

    // Ensure branch exists
    ensure_branch_exists(api_base, token, full_name, branch, Duration::from_secs(30)).await?;

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

    let contexts_vec: Vec<String> = required_contexts.iter().map(|s| s.to_string()).collect();
    let body = BranchProtectionRequest {
        required_status_checks: RequiredStatusChecks {
            strict: true,
            contexts: &contexts_vec,
        },
        enforce_admins: true,
        required_pull_request_reviews: RequiredPullRequestReviews {
            required_approving_review_count: 1,
            dismiss_stale_reviews: true,
            require_code_owner_reviews: false,
            require_last_push_approval: true,
        },
        restrictions: None,
        allow_force_pushes: false,
        allow_deletions: false,
        required_linear_history: true,
        block_creations: false,
        required_conversation_resolution: true,
        lock_branch: false,
        allow_fork_syncing: false,
    };

    debug!("PUT branch protection (with checks) payload prepared");
    let resp = client.put(url).json(&body).send().await?;
    let status = resp.status();
    if status.is_success() {
        info!("Branch protection (with checks) applied");
        return Ok(());
    }

    let text = resp
        .text()
        .await
        .unwrap_or_else(|_| "<no body>".to_string());
    warn!(
        "Failed to apply branch protection (with checks) {}: {}",
        status,
        text.trim()
    );
    Err(anyhow!(format!(
        "Failed to apply branch protection (with checks) (status {}): {}",
        status,
        text.trim()
    )))
}

async fn ensure_branch_exists(
    api_base: &str,
    token: &str,
    full_name: &str,
    branch: &str,
    max_wait: Duration,
) -> Result<()> {
    let (owner, repo) = split_template_name(full_name)?;
    let url = format!(
        "{}/repos/{}/{}/branches/{}",
        api_base.trim_end_matches('/'),
        owner,
        repo,
        branch
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

    let start = tokio::time::Instant::now();
    let mut delay = Duration::from_millis(400);
    loop {
        let resp = client.get(&url).send().await?;
        match resp.status().as_u16() {
            200 => {
                debug!("Branch '{}' is available", branch);
                return Ok(());
            }
            404 => {
                if start.elapsed() >= max_wait {
                    return Err(anyhow!(
                        "Default branch '{}' was not found within {:?}",
                        branch,
                        max_wait
                    ));
                }
                debug!("Branch '{}' not found yet, retrying...", branch);
                sleep(delay).await;
                delay = std::cmp::min(delay * 2, Duration::from_secs(2));
            }
            401 | 403 => {
                let text = resp.text().await.unwrap_or_default();
                return Err(anyhow!(
                    "Insufficient permission to check branch existence: {}",
                    text
                ));
            }
            code => {
                let text = resp.text().await.unwrap_or_default();
                warn!(
                    "Unexpected response {} while checking branch: {}",
                    code, text
                );
                if start.elapsed() >= max_wait {
                    return Err(anyhow!(
                        "Failed to confirm branch existence within {:?}: {} {}",
                        max_wait,
                        code,
                        text
                    ));
                }
                sleep(delay).await;
                delay = std::cmp::min(delay * 2, Duration::from_secs(2));
            }
        }
    }
}

#[derive(Deserialize)]
struct GitRef {
    #[serde(rename = "object")]
    obj: GitObject,
}

#[derive(Deserialize)]
struct GitObject {
    sha: String,
}

pub async fn create_branch_from_base(
    api_base: &str,
    token: &str,
    full_name: &str,
    base_branch: &str,
    new_branch: &str,
) -> Result<()> {
    let (owner, repo) = split_template_name(full_name)?;
    let base_ref_url = format!(
        "{}/repos/{}/{}/git/ref/heads/{}",
        api_base.trim_end_matches('/'),
        owner,
        repo,
        base_branch
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
        .default_headers(headers.clone())
        .build()?;

    // Get base branch SHA
    let base_resp = client.get(&base_ref_url).send().await?;
    if !base_resp.status().is_success() {
        let text = base_resp.text().await.unwrap_or_default();
        return Err(anyhow!(format!(
            "Failed to read base branch '{}': {}",
            base_branch, text
        )));
    }
    let base_ref: GitRef = base_resp.json().await?;
    let sha = base_ref.obj.sha;

    // Create new ref
    let create_ref_url = format!(
        "{}/repos/{}/{}/git/refs",
        api_base.trim_end_matches('/'),
        owner,
        repo
    );
    let payload = serde_json::json!({
        "ref": format!("refs/heads/{}", new_branch),
        "sha": sha
    });
    let create_resp = client.post(create_ref_url).json(&payload).send().await?;
    if create_resp.status().is_success() || create_resp.status().as_u16() == 201 {
        info!("Created branch '{}'", new_branch);
        Ok(())
    } else {
        let text = create_resp.text().await.unwrap_or_default();
        Err(anyhow!(format!(
            "Failed to create branch '{}': {}",
            new_branch, text
        )))
    }
}

#[derive(Serialize)]
struct DeploymentBranchPolicy {
    protected_branches: bool,
    custom_branch_policies: bool,
}

#[derive(Serialize)]
struct EnvironmentRequest {
    deployment_branch_policy: DeploymentBranchPolicy,
}

pub async fn ensure_environment_with_branches(
    api_base: &str,
    token: &str,
    full_name: &str,
    env_name: &str,
    allowed_patterns: &[&str],
) -> Result<()> {
    let (owner, repo) = split_template_name(full_name)?;
    let base = api_base.trim_end_matches('/');
    let env_url = format!(
        "{}/repos/{}/{}/environments/{}",
        base, owner, repo, env_name
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
        .default_headers(headers.clone())
        .build()?;

    // Enable custom branch policies
    let body = EnvironmentRequest {
        deployment_branch_policy: DeploymentBranchPolicy {
            protected_branches: false,
            custom_branch_policies: true,
        },
    };
    let resp = client.put(&env_url).json(&body).send().await?;
    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!(format!(
            "Failed to create/update environment '{}': {}",
            env_name, text
        )));
    }

    // Add allowed branch patterns
    let policy_url = format!("{}/deployment-branch-policies", env_url);
    for pat in allowed_patterns {
        let payload = serde_json::json!({ "name": pat });
        let add = client.post(&policy_url).json(&payload).send().await?;
        if !add.status().is_success() && add.status().as_u16() != 201 {
            let text = add.text().await.unwrap_or_default();
            warn!(
                "Failed to add branch policy '{}' to env '{}': {}",
                pat, env_name, text
            );
        } else {
            info!(
                "Environment '{}' allows branches matching '{}'",
                env_name, pat
            );
        }
    }
    Ok(())
}

#[derive(Deserialize)]
struct TreeEntry {
    path: String,
    r#type: String,
    sha: String,
}

#[derive(Deserialize)]
struct GitTree {
    tree: Vec<TreeEntry>,
}

#[derive(Deserialize)]
struct RepoInfo {
    default_branch: String,
}

#[derive(Deserialize)]
struct Blob {
    content: String,
    encoding: String,
}

pub async fn get_repo_default_branch(
    api_base: &str,
    token: &str,
    full_name: &str,
) -> Result<String> {
    let (owner, repo) = split_template_name(full_name)?;
    let url = format!(
        "{}/repos/{}/{}",
        api_base.trim_end_matches('/'),
        owner,
        repo
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
    let resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!(format!(
            "Failed to read repo info for '{}': {}",
            full_name, text
        )));
    }
    let info: RepoInfo = resp.json().await?;
    Ok(info.default_branch)
}

pub async fn copy_dirs_from_repo(
    api_base: &str,
    token: &str,
    source_full_name: &str,
    target_full_name: &str,
    target_branch: &str,
    dir_prefixes: &[&str],
) -> Result<()> {
    let (src_owner, src_repo) = split_template_name(source_full_name)?;
    let (dst_owner, dst_repo) = split_template_name(target_full_name)?;
    let source_default = get_repo_default_branch(api_base, token, source_full_name).await?;
    let base = api_base.trim_end_matches('/');

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
        .default_headers(headers.clone())
        .build()?;

    // Fetch source tree recursively
    let tree_url = format!(
        "{}/repos/{}/{}/git/trees/{}?recursive=1",
        base, src_owner, src_repo, source_default
    );
    let tree_resp = client.get(&tree_url).send().await?;
    if !tree_resp.status().is_success() {
        let text = tree_resp.text().await.unwrap_or_default();
        return Err(anyhow!(format!("Failed to read source tree: {}", text)));
    }
    let tree: GitTree = tree_resp.json().await?;

    for entry in tree.tree.iter().filter(|e| e.r#type == "blob") {
        if !dir_prefixes.iter().any(|p| entry.path.starts_with(p)) {
            continue;
        }
        // Fetch blob
        let blob_url = format!(
            "{}/repos/{}/{}/git/blobs/{}",
            base, src_owner, src_repo, entry.sha
        );
        let blob_resp = client.get(&blob_url).send().await?;
        if !blob_resp.status().is_success() {
            let text = blob_resp.text().await.unwrap_or_default();
            warn!("Failed to fetch blob for '{}': {}", entry.path, text);
            continue;
        }
        let blob: Blob = blob_resp.json().await?;
        if blob.encoding.to_lowercase() != "base64" {
            warn!(
                "Unexpected blob encoding for '{}': {}",
                entry.path, blob.encoding
            );
            continue;
        }
        let content_b64 = blob.content.replace('\n', "");

        // Skip if exists
        let get_content_url = format!(
            "{}/repos/{}/{}/contents/{}?ref={}",
            base, dst_owner, dst_repo, entry.path, target_branch
        );
        let exists = client.get(&get_content_url).send().await?;
        if exists.status().is_success() {
            debug!("Skipping existing '{}'", entry.path);
            continue;
        }

        // PUT file
        let put_url = format!(
            "{}/repos/{}/{}/contents/{}",
            base, dst_owner, dst_repo, entry.path
        );
        let payload = serde_json::json!({
            "message": format!("chore: seed {} from service-template", entry.path),
            "content": content_b64,
            "branch": target_branch
        });
        let put_resp = client.put(&put_url).json(&payload).send().await?;
        if !put_resp.status().is_success() && put_resp.status().as_u16() != 201 {
            let text = put_resp.text().await.unwrap_or_default();
            warn!("Failed to write '{}': {}", entry.path, text);
        } else {
            info!("Seeded '{}'", entry.path);
        }
    }
    Ok(())
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
