# GitHub Client (Rust) - Create Repo from Template

CLI to create a new GitHub repository from a template repository using the GitHub API.

## Inputs
- **repo_name**: name of the new repository (string)
- **repo_desc**: description (string)
- **repo_type**: `public` or `private`
- **template_name**: template in the form `owner/repo` (string)
- **branch**: include all branches from the template (boolean)
- **protect_default_branch**: whether to protect the default branch after creation (boolean, default true)

## Auth Token Requirements
Provide a GitHub token via `GITHUB_TOKEN` or `GH_TOKEN` with permissions to:
- Read the template repository (and its branches).
- Create repositories under the authenticated user or organization.

Recommended:
- For public repos: a token with `public_repo`.
- For private repos: a token with `repo`.
- For organizations: make sure the token has permission to create repositories in that org, and the org policy allows template usage.

Fine-grained PAT (recommended):
- Repository permissions (on your account/org):
  - Administration: Read and write (needed for branch protection and branch creation)
  - Environments: Read and write (needed to configure environments)
  - Metadata: Read
- On the template repository:
  - Contents: Read

## Usage (local)
```bash
# Environment variables (recommended)
export GITHUB_TOKEN=ghp_your_pat_here
export REPO_NAME="my-new-repo"
export REPO_DESC="My repo from template"
export REPO_TYPE="private" # or public
export TEMPLATE_NAME="owner/template-repo"
export BRANCH="false"      # set to true to include all branches
export PROTECT_DEFAULT_BRANCH="true" # toggle default-branch protection

cargo run --release --
```

Or via flags:
```bash
cargo run --release -- \
  --repo-name my-new-repo \
  --repo-desc "My repo from template" \
  --repo-type private \
  --template-name owner/template-repo \
  --branch \
  --protect-default-branch
```

## GitHub Actions
This repository includes a workflow `create-repo.yml` with `workflow_dispatch` inputs. Trigger it from the Actions tab and provide:
- repo_name, repo_desc, repo_type, template_name, branch, protect_default_branch

Set a repository secret `GH_PAT` with an appropriate Personal Access Token (see above). The workflow will use this token to create the repository.

## Branch Protection
- The CLI applies branch protection to the repository’s default branch when `protect_default_branch=true`.
- It waits briefly for the default branch to be fully available to avoid 404 race conditions, then sets protection rules:
  - PR required with at least 1 approval
  - Dismiss stale reviews, require last push approval
  - Enforce admins
  - Disallow force-pushes and deletions
  - Require linear history and conversation resolution

## Service templates: GitFlow and environments
When the template repository name starts with `service-` (e.g., `service-golang`, `service-rust`), the CLI auto-configures:
- Branches:
  - Creates `dev` from the default branch.
  - Applies protection to `dev` (same policy as default) if protection is enabled.
- Environments:
  - `dev` environment allows branches `dev`, `feature/*`, `hotfix/*`.
  - `release` environment allows branches `release/*`, `main`.

This makes it easy to follow a GitFlow-style workflow across service repositories created from standard service templates.