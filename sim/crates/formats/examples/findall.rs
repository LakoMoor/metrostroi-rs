use formats::gma::Gma;
use std::path::Path;
fn main() {
    let pat: Vec<String> = std::env::args().skip(1).collect();
    for d in std::fs::read_dir(r"E:\Games\steamapps\workshop\content\4000").unwrap().flatten() {
        for f in std::fs::read_dir(d.path()).into_iter().flatten().flatten() {
            let p = f.path();
            if p.extension().is_some_and(|x| x == "gma" || x == "bin") {
                if let Ok(g) = Gma::open(&p) {
                    let hits: Vec<_> = g.entries.iter().filter(|e| pat.iter().any(|q| e.path.contains(q.as_str()))).map(|e| e.path.clone()).collect();
                    if !hits.is_empty() { println!("{} {}: {:?}", d.file_name().to_string_lossy(), Path::new(&p).file_name().unwrap().to_string_lossy(), &hits[..hits.len().min(4)]); }
                }
            }
        }
    }
}
