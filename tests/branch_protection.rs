use github_client::protect_branch;
use httpmock::prelude::*;

#[tokio::test]
async fn protects_branch_successfully() {
    let server = MockServer::start();
    let owner = "me";
    let repo = "new-repo";
    let branch = "main";
    let token = "testtoken";

    let _m = server.mock(|when, then| {
        when.method(PUT)
            .path(format!(
                "/repos/{}/{}/branches/{}/protection",
                owner, repo, branch
            ))
            .header("authorization", "Bearer testtoken")
            .header("accept", "application/vnd.github+json")
            .header("x-github-api-version", "2022-11-28")
            .json_body_obj(&serde_json::json!({
                "required_status_checks": {
                    "strict": true,
                    "contexts": []
                },
                "enforce_admins": true,
                "required_pull_request_reviews": {
                    "required_approving_review_count": 1,
                    "dismiss_stale_reviews": true,
                    "require_code_owner_reviews": false,
                    "require_last_push_approval": true
                },
                "restrictions": null,
                "allow_force_pushes": false,
                "allow_deletions": false,
                "required_linear_history": true,
                "block_creations": false,
                "required_conversation_resolution": true,
                "lock_branch": false,
                "allow_fork_syncing": false
            }));
        then.status(200);
    });

    let api_base = server.base_url();
    let res = protect_branch(&api_base, token, &format!("{}/{}", owner, repo), branch).await;
    assert!(res.is_ok());
}

#[tokio::test]
async fn protect_branch_returns_error_on_forbidden() {
    let server = MockServer::start();
    let owner = "me";
    let repo = "new-repo";
    let branch = "main";
    let token = "testtoken";

    let _m = server.mock(|when, then| {
        when.method(PUT).path(format!(
            "/repos/{}/{}/branches/{}/protection",
            owner, repo, branch
        ));
        then.status(403).body("{\"message\":\"forbidden\"}");
    });

    let api_base = server.base_url();
    let res = protect_branch(&api_base, token, &format!("{}/{}", owner, repo), branch).await;
    assert!(res.is_err());
}
