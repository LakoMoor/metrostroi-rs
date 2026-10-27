use formats::vfs::Vfs;
use std::path::Path;
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut v = Vfs::default();
    for id in ["1812142201","1095094174","1095098251","1095100683","1095105863","1095109617","1095111608"] {
        let d = Path::new(r"E:\Games\steamapps\workshop\content\4000").join(id);
        let g = std::fs::read_dir(&d).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x == "gma")).unwrap();
        v.mount(&g).unwrap();
    }
    if a.len() == 2 && a[0] == "cat" {
        print!("{}", String::from_utf8_lossy(v.read(&a[1]).unwrap()));
    } else {
        for p in v.paths().filter(|p| p.contains(&a[0])) { println!("{p}"); }
    }
}
