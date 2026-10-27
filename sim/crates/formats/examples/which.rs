use formats::gma::Gma;
use std::path::{Path, PathBuf};
// usage: which <substring> <gma-or-dir>...  — prints title and number of matching entries per archive (dirs are scanned one level deep)
fn collect(p: &Path, out: &mut Vec<PathBuf>, depth: u32) {
    if p.is_dir() {
        if depth > 1 {
            return;
        }
        if let Ok(rd) = std::fs::read_dir(p) {
            for e in rd.flatten() {
                collect(&e.path(), out, depth + 1);
            }
        }
    } else if p.extension().is_some_and(|x| x == "gma" || x == "bin") {
        out.push(p.to_path_buf());
    }
}
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut files = Vec::new();
    for f in &a[1..] {
        collect(Path::new(f), &mut files, 0);
    }
    for f in files {
        if let Ok(g) = Gma::open(&f) {
            let n = g.entries.iter().filter(|e| e.path.contains(&a[0])).count();
            if n > 0 {
                println!("{}: \"{}\" {} files, {n} match", f.display(), g.title, g.entries.len());
            }
        }
    }
}
