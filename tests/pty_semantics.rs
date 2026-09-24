#![cfg(unix)]
#[test]
fn manga_real_keys_reconstruct_selection_and_denial_not_just_exit() {
    let output=std::process::Command::new("python3")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["-c","import json,sys;sys.path.insert(0,'scripts');from pty_smoke import capture;print(json.dumps(capture(sys.argv[1],56,24,'mono',True)))",env!("CARGO_BIN_EXE_manga")])
        .output().expect("bounded Python PTY helper");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let record: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let hex = record["raw_hex"].as_str().unwrap();
    let raw: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    let leave = b"\x1b[?1049l";
    let end = raw
        .windows(leave.len())
        .position(|b| b == leave)
        .expect("restore");
    let mut parser = vt100::Parser::new(24, 56, 0);
    parser.process(&raw[..end]);
    let contents = parser.screen().contents();
    assert!(contents.contains("> 2 SCOUT"), "{contents}");
    assert!(
        contents.contains("Permission explicitly declined"),
        "{contents}"
    );
}
