use std::env;

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use github_client::RepoResponse;

#[derive(Parser, Debug)]
#[command(
    name = "github-client",
    version,
    about = "Create a repo from a GitHub template"
)]
struct Opts {
    /// Repository name to create
    #[arg(long, env = "REPO_NAME")]
    repo_name: String,

    /// Repository description
    #[arg(long, env = "REPO_DESC")]
    repo_desc: String,

    /// Repository type: public | private
    #[arg(long, env = "REPO_TYPE", value_parser = ["public", "private"])]
    repo_type: String,

    /// Template repository in the form 'owner/repo'
    #[arg(long, env = "TEMPLATE_NAME")]
    template_name: String,

    /// Include all branches from template (true/false)
    #[arg(long, env = "BRANCH", default_value_t = false)]
    branch: bool,

    /// GitHub API base URL, defaults to public GitHub
    #[arg(long, env = "GITHUB_API_URL", default_value = "https://api.github.com")]
    api_base: String,

    /// GitHub token; falls back to GH_TOKEN if not set
    #[arg(long, env = "GITHUB_TOKEN")]
    token: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let opts = Opts::parse();
    let token = resolve_token(opts.token.as_deref())?;
    let is_private = opts.repo_type.eq_ignore_ascii_case("private");

    let repo: RepoResponse = github_client::generate_from_template(
        &opts.api_base,
        &token,
        &opts.template_name,
        &opts.repo_name,
        &opts.repo_desc,
        is_private,
        opts.branch,
    )
    .await
    .context("Failed to call GitHub API")?;

    println!(
        "{{\"full_name\":\"{}\",\"html_url\":\"{}\",\"default_branch\":\"{}\"}}",
        repo.full_name, repo.html_url, repo.default_branch
    );
    Ok(())
}

fn resolve_token(primary: Option<&str>) -> Result<String> {
    if let Some(t) = primary {
        if !t.is_empty() {
            return Ok(t.to_string());
        }
    }
    if let Ok(t) = env::var("GH_TOKEN") {
        if !t.is_empty() {
            return Ok(t);
        }
    }
    Err(anyhow!(
        "Missing token. Provide via --token, GITHUB_TOKEN, or GH_TOKEN env var"
    ))
}
