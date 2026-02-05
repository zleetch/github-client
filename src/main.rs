use std::env;

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use github_client::RepoResponse;
use tracing::{debug, info};
use tracing_subscriber::EnvFilter;

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

    /// Apply branch protection to the default branch after creation
    #[arg(long, env = "PROTECT_DEFAULT_BRANCH", default_value_t = true)]
    protect_default_branch: bool,

    /// Override source for seeding service-* scaffolding (default: <owner>/service-template)
    #[arg(long, env = "SERVICE_TEMPLATE_REPO")]
    service_template_repo: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging with RUST_LOG or default to info
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .compact()
        .try_init();

    let opts = Opts::parse();
    info!("Starting GitHub template generation");
    debug!(
        "Parsed options: repo_name='{}', repo_type='{}', template='{}', branch={}",
        opts.repo_name, opts.repo_type, opts.template_name, opts.branch
    );

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
    info!("Repository created: {}", repo.full_name);

    // Detect service-* template name
    let is_service = opts
        .template_name
        .rsplit('/')
        .next()
        .map(|n| n.starts_with("service-"))
        .unwrap_or(false);

    // If service-*, seed terraform/ and helm/ from service-template before branch protection/dev creation
    if is_service {
        let owner = repo
            .full_name
            .split('/')
            .next()
            .unwrap_or_default()
            .to_string();
        let source_full_name = opts
            .service_template_repo
            .clone()
            .unwrap_or_else(|| format!("{}/service-template", owner));
        info!(
            "Seeding 'terraform/' and 'helm/' from {} into {}",
            source_full_name, repo.full_name
        );
        github_client::copy_dirs_from_repo(
            &opts.api_base,
            &token,
            &source_full_name,
            &repo.full_name,
            &repo.default_branch,
            &["terraform/", "helm/", "kustomize/"],
        )
        .await
        .context("Failed to seed content from service-template")?;
    }

    // Optionally apply branch protection to the default branch
    if opts.protect_default_branch {
        if is_service {
            github_client::protect_branch_with_checks(
                &opts.api_base,
                &token,
                &repo.full_name,
                &repo.default_branch,
                &["branch-policy"],
            )
            .await
            .context("Failed to apply branch protection with checks")?;
        } else {
            github_client::protect_branch(
                &opts.api_base,
                &token,
                &repo.full_name,
                &repo.default_branch,
            )
            .await
            .context("Failed to apply branch protection")?;
        }
        info!(
            "Branch protection applied on '{}:{}'",
            repo.full_name, repo.default_branch
        );
    } else {
        info!("Skipping branch protection as requested");
    }

    // Auto-setup gitflow and environments for service-* templates
    if opts
        .template_name
        .rsplit('/')
        .next()
        .map(|n| n.starts_with("service-"))
        .unwrap_or(false)
    {
        info!("Detected service-* template; setting up gitflow branches and environments");
        // Create 'dev' branch from default
        github_client::create_branch_from_base(
            &opts.api_base,
            &token,
            &repo.full_name,
            &repo.default_branch,
            "dev",
        )
        .await
        .context("Failed to create 'dev' branch")?;

        // Protect 'dev' branch as well
        if opts.protect_default_branch {
            github_client::protect_branch_with_checks(
                &opts.api_base,
                &token,
                &repo.full_name,
                "dev",
                &["branch-policy"],
            )
            .await
            .context("Failed to protect 'dev' branch")?;
        }

        // Environments
        github_client::ensure_environment_with_branches(
            &opts.api_base,
            &token,
            &repo.full_name,
            "dev",
            &["dev", "feature/*", "hotfix/*"],
        )
        .await
        .context("Failed to configure 'dev' environment")?;

        github_client::ensure_environment_with_branches(
            &opts.api_base,
            &token,
            &repo.full_name,
            "release",
            &["release/*", "main"],
        )
        .await
        .context("Failed to configure 'release' environment")?;

        info!("Gitflow branches and environments configured");
    }
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
