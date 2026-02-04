use github_client::generate_from_template;
use httpmock::prelude::*;

#[tokio::test]
async fn creates_repo_successfully() {
    let server = MockServer::start();
    let owner = "owner";
    let template = "template";
    let token = "testtoken";
    let repo_name = "new-repo";
    let repo_desc = "desc";

    let _m = server.mock(|when, then| {
        when.method(POST)
            .path(format!("/repos/{}/{}/generate", owner, template))
            .header("authorization", "Bearer testtoken")
            .header("accept", "application/vnd.github+json")
            .header("x-github-api-version", "2022-11-28")
            .json_body_obj(&serde_json::json!({
                "name": repo_name,
                "description": repo_desc,
                "private": true,
                "include_all_branches": false
            }));
        then.status(201)
            .json_body_obj(&serde_json::json!({
                "full_name": format!("{}/{}", owner, repo_name),
                "html_url": format!("https://github.com/{}/{}", owner, repo_name),
                "default_branch": "main"
            }));
    });

    let api_base = server.base_url();
    let template_name = format!("{}/{}", owner, template);
    let res = generate_from_template(
        &api_base,
        token,
        &template_name,
        repo_name,
        repo_desc,
        true,
        false,
    )
    .await
    .expect("should succeed");

    assert_eq!(res.full_name, format!("{}/{}", owner, repo_name));
}

#[tokio::test]
async fn returns_error_on_api_failure() {
    let server = MockServer::start();
    let owner = "owner";
    let template = "template";
    let token = "testtoken";
    let repo_name = "new-repo";
    let repo_desc = "desc";

    let _m = server.mock(|when, then| {
        when.method(POST)
            .path(format!("/repos/{}/{}/generate", owner, template));
        then.status(404).body("not found");
    });

    let api_base = server.base_url();
    let template_name = format!("{}/{}", owner, template);
    let res = generate_from_template(
        &api_base,
        token,
        &template_name,
        repo_name,
        repo_desc,
        false,
        true,
    )
    .await;

    assert!(res.is_err());
}
