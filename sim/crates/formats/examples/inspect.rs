use formats::{mdl, vfs::Vfs};
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let ws = Path::new(r"E:\Games\steamapps\workshop\content\4000");
    let packs = ["1812142201", "1095094174", "1095098251", "1095100683", "1095105863", "1095109617", "1095111608"];
    let t = std::time::Instant::now();
    let mut vfs = Vfs::default();
    for id in packs {
        let dir = ws.join(id);
        let gma = std::fs::read_dir(&dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .find(|p| p.extension().map_or(false, |x| x == "gma"))
            .expect("gma");
        vfs.mount(&gma)?;
    }
    println!("mounted {} files in {:?}", vfs.len(), t.elapsed());

    let name = std::env::args().nth(1).unwrap_or("models/metrostroi_train/81-717/81-717_mvm".into());
    let mdl_b = vfs.read(&format!("{name}.mdl")).expect("mdl");
    let vvd_b = vfs.read(&format!("{name}.vvd")).expect("vvd");
    let vtx_b = vfs.read(&format!("{name}.dx90.vtx")).expect("vtx");
    let t = std::time::Instant::now();
    let m = mdl::load(mdl_b, vvd_b, vtx_b)?;
    println!("loaded v{} in {:?}; cd={:?}", m.version, t.elapsed(), m.cd_textures);
    for bp in &m.body_parts {
        for (i, sm) in bp.models.iter().enumerate() {
            let (mut v, mut ix) = (0, 0);
            let (mut mn, mut mx) = ([f32::MAX; 3], [f32::MIN; 3]);
            for me in &sm.meshes {
                v += me.vertices.len();
                ix += me.indices.len();
                for p in &me.vertices {
                    for a in 0..3 {
                        mn[a] = mn[a].min(p.pos[a]);
                        mx[a] = mx[a].max(p.pos[a]);
                    }
                }
            }
            println!("  bodypart '{}' model {i}: meshes={} verts={v} tris={} size(m)={:.2} x {:.2} x {:.2}",
                bp.name, sm.meshes.len(), ix / 3,
                (mx[0]-mn[0])*0.0254, (mx[1]-mn[1])*0.0254, (mx[2]-mn[2])*0.0254);
            for me in &sm.meshes { println!("      material {}", me.material); }
        }
    }
    Ok(())
}
