#![cfg(unix)]

#[test]
fn actual_holdout_keys_resize_and_restore_reconstruct_selected_objects() {
    for (binary, witnesses) in [
        (env!("CARGO_BIN_EXE_foldroom"), &["SELECTED 2", "FRONT"][..]),
        (env!("CARGO_BIN_EXE_cuebox"), &["Bram", "Q04"][..]),
        (
            env!("CARGO_BIN_EXE_weavebench"),
            &["THREADING", "Shaft"][..],
        ),
    ] {
        // Frozen capture sends Space, Tab, n, resize, Esc. The host is paused,
        // so selection/action witnesses do not depend on animation timing.
        let output = std::process::Command::new("python3")
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args(["-c", "import json,sys;sys.path.insert(0,'scripts');from pty_smoke import capture;print(json.dumps(capture(sys.argv[1],56,24,'mono',True)))", binary])
            .output().expect("bounded PTY capture");
        assert!(
            output.status.success(),
            "{binary}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let record: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let hex = record["raw_hex"].as_str().unwrap();
        let raw: Vec<_> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        let leave = b"\x1b[?1049l";
        let end = raw.windows(leave.len()).position(|b| b == leave).unwrap();
        let mut parser = vt100::Parser::new(24, 56, 0);
        parser.process(&raw[..end]);
        let contents = parser.screen().contents();
        for witness in witnesses {
            assert!(
                contents.contains(witness),
                "{binary}: missing {witness}\n{contents}"
            );
        }
    }
}
