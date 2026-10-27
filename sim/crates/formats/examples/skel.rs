use formats::vfs::Vfs;
use std::path::Path;
fn main() {
    let mut v = Vfs::default();
    for id in ["1812142201","1095094174","1095098251","1095100683","1095105863","1095109617","1095111608"] {
        let d = Path::new(r"E:\Games\steamapps\workshop\content\4000").join(id);
        let g = std::fs::read_dir(&d).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x == "gma")).unwrap();
        v.mount(&g).unwrap();
    }
    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(r"E:\metrostroi2pz\out\controls\717_mvm.json").unwrap()).unwrap();
    let mut seen = std::collections::BTreeSet::new();
    for (_, p) in json["client_props"].as_object().unwrap() {
        let Some(m) = p["model"].as_str() else { continue };
        let m = m.trim_end_matches(".mdl").to_string();
        if !seen.insert(m.clone()) { continue; }
        let (Some(a), Some(b), Some(c)) = (v.read(&format!("{m}.mdl")), v.read(&format!("{m}.vvd")), v.read(&format!("{m}.dx90.vtx"))) else { continue };
        let Ok(model) = formats::mdl::load(a, b, c) else { continue };
        if let Some(s) = &model.skeleton {
            let pp: Vec<_> = s.pose_params.iter().map(|p| format!("{}[{}..{}]", p.name, p.start, p.end)).collect();
            println!("{m}: {} bones, params {:?}", s.bones.len(), pp);
        }
    }
}
