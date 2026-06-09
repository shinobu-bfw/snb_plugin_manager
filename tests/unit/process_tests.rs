use super::*;

#[test]
fn github_repo_dir_name_reads_https_url() {
    assert_eq!(
        github_repo_dir_name("https://github.com/owner/my-plugin.git").unwrap(),
        "my-plugin"
    );
}

#[test]
fn github_repo_dir_name_reads_ssh_url() {
    assert_eq!(
        github_repo_dir_name("git@github.com:owner/my-plugin.git").unwrap(),
        "my-plugin"
    );
}

#[test]
fn github_repo_dir_name_rejects_non_github_url() {
    assert!(github_repo_dir_name("https://example.com/owner/repo.git").is_err());
}

#[test]
fn github_repo_dir_name_rejects_subpath_url() {
    assert!(github_repo_dir_name("https://github.com/owner/repo/tree/main").is_err());
}
