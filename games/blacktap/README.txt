Assassin's Keg: Black Tap — game data. Every .toml, .abc and .txt file here is embedded into the
game by crates/blacktap/build.rs and read with blacktap::data("<relative path>"); running from
the repository root, the files on disk win (edit, then `load`, no rebuild).
See docs/CONTENT_GUIDE.md for how to add each kind of content.
