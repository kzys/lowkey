pub fn die(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(1);
}
