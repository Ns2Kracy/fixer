use std::process::Command;

#[test]
fn providers_list_advertises_bangumi_and_local_anime() {
    let output = Command::new(env!("CARGO_BIN_EXE_fixer"))
        .args(["providers", "list"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("local\tmovie,television,anime,music,book\toffline"));
    assert!(stdout.contains("bangumi\tanime\tnetwork"));
}
