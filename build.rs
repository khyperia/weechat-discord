// build.rs

// Bring in a dependency on an externally maintained `gcc` package which manages
// invoking the C compiler.
extern crate pkg_config;
extern crate gcc;

fn main() {
    let mut config = gcc::Config::new();
    let weechat = pkg_config::probe_library("weechat").unwrap();
    for path in weechat.include_paths {
        config.include(path);
    }
    config.file("src/weecord.c");
    config.compile("libweecord.a");
}
