fn main() {
    if let Err(error) = mpv_tidy_lib::run() {
        eprintln!("Anime Subtitle Manager failed to start: {error}");
        std::process::exit(1);
    }
}
