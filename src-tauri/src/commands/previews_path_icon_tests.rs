use std::path::Path;

#[test]
fn missing_path_returns_no_icon() {
    let cache_dir = std::env::temp_dir().join("orange-path-icon-test");
    let missing = cache_dir.join("missing-file.lnk");
    let result = super::previews::icon_data_url_for_path(Path::new(&missing), &cache_dir)
        .expect("missing path should not be an error");
    assert!(result.is_none());
}
