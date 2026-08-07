use summoner::claude::munge_project_dir;

#[test]
fn munge_replaces_non_alphanumeric_with_dashes() {
    assert_eq!(munge_project_dir("/home/siim/dev/summoner"), "-home-siim-dev-summoner");
    assert_eq!(munge_project_dir("/home/u/my_proj.v2"), "-home-u-my-proj-v2");
    assert_eq!(munge_project_dir(""), "");
}
