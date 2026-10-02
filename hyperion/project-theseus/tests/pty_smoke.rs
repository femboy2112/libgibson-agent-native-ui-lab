use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::{Read, Write};
use std::time::Duration;

#[test]
fn test_pty_smoke() {
    let pty_system = NativePtySystem::default();
    let size = PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    };
    let pair = pty_system.openpty(size).unwrap();

    let mut cmd = CommandBuilder::new("cargo");
    cmd.args(["run", "--bin", "project-theseus"]);

    let mut child = pair.slave.spawn_command(cmd).unwrap();
    
    let mut reader = pair.master.try_clone_reader().unwrap();
    let mut writer = pair.master.take_writer().unwrap();

    std::thread::sleep(Duration::from_millis(2000));

    let inputs = b"ffw1234r";
    writer.write_all(inputs).unwrap();
    
    pair.master.resize(PtySize { rows: 40, cols: 120, pixel_width: 0, pixel_height: 0 }).unwrap();
    std::thread::sleep(Duration::from_millis(1000));

    let mut buf = [0; 8192];
    let n = reader.read(&mut buf).unwrap_or(0);
    let output = String::from_utf8_lossy(&buf[..n]);
    assert!(output.len() > 0);
    
    writer.write_all(b"q").unwrap();
    std::thread::sleep(Duration::from_millis(1000));
    
    let success = child.try_wait().unwrap().is_some();
    if !success {
        child.kill().unwrap();
    }
}
