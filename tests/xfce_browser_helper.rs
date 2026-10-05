use std::process::Command;

#[test]
fn xfce_browser_helper_fixture_validation() {
    let status = Command::new("sh")
        .arg("tests/test-xfce-browser-helper.sh")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()
        .expect("run isolated XFCE browser helper fixtures");
    assert!(status.success(), "XFCE browser helper fixtures failed");
}
