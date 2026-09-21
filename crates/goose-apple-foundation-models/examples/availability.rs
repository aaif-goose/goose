fn main() {
    match goose_apple_foundation_models::model_info() {
        Ok(info) => println!(
            "Apple Foundation Models available ({} token context)",
            info.context_size
        ),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
