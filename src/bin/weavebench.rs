fn main() -> Result<(), Box<dyn std::error::Error>> {
    gibson_ui_lab::ui::run(gibson_ui_lab::weavebench::Weavebench::default())
}
