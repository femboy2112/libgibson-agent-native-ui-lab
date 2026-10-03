fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match project_pulsar::cli::main_with(&args) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("project-pulsar: {e}");
            std::process::exit(2);
        }
    }
}
