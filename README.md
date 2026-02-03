# GitHub Client (Rust) - Create Repo from Template

CLI to create a new GitHub repository from a template repository using the GitHub API.

## Inputs
- **repo_name**: name of the new repository (string)
- **repo_desc**: description (string)
- **repo_type**: `public` or `private`
- **template_name**: template in the form `owner/repo` (string)
- **branch**: include all branches from the template (boolean)

## Auth Token Requirements
Provide a GitHub token via `GITHUB_TOKEN` or `GH_TOKEN` with permissions to:
- Read the template repository (and its branches).
- Create repositories under the authenticated user or organization.

Recommended:
- For public repos: a token with `public_repo`.
- For private repos: a token with `repo`.
- For organizations: make sure the token has permission to create repositories in that org, and the org policy allows template usage.

## Usage (local)
```bash
# Environment variables (recommended)
export GITHUB_TOKEN=ghp_your_pat_here
export REPO_NAME="my-new-repo"
export REPO_DESC="My repo from template"
export REPO_TYPE="private" # or public
export TEMPLATE_NAME="owner/template-repo"
export BRANCH="false"      # set to true to include all branches

cargo run --release --
```

Or via flags:
```bash
cargo run --release -- \
  --repo-name my-new-repo \
  --repo-desc "My repo from template" \
  --repo-type private \
  --template-name owner/template-repo \
  --branch
```

## GitHub Actions
This repository includes a workflow `create-repo.yml` with `workflow_dispatch` inputs. Trigger it from the Actions tab and provide:
- repo_name, repo_desc, repo_type, template_name, branch

Set a repository secret `GH_PAT` with an appropriate Personal Access Token (see above). The workflow will use this token to create the repository.


