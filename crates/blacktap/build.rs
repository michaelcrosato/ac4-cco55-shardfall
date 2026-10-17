// Embeds every data file under /games/blacktap (TOML, ABC songs, text) into the binary,
// keyed by its path relative to that folder ("missions/r1.toml"), so the browser and shipped
// executables carry the game's data. Running from the repository's root, the files on disk win
// (`blacktap::data`), so edits show at the next `load` with no rebuild.
use std::io::Write;

fn collect(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(&p, out);
        } else if p.extension().is_some_and(|x| x == "toml" || x == "abc" || x == "txt") {
            out.push(p);
        }
    }
}

fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/blacktap");
    assert!(root.is_dir(), "{} is missing: blacktap embeds the repository's /games/blacktap folder", root.display());
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files = Vec::new();
    collect(&root, &mut files);
    files.sort();
    let out = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("blacktap_data.rs");
    let mut f = std::fs::File::create(out).unwrap();
    writeln!(f, "pub const DATA: &[(&str, &str)] = &[").unwrap();
    for p in files {
        println!("cargo:rerun-if-changed={}", p.display());
        let rel = p.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
        let abs = p.canonicalize().unwrap();
        writeln!(f, "    ({rel:?}, include_str!({:?})),", abs.display().to_string()).unwrap();
    }
    writeln!(f, "];").unwrap();
}
