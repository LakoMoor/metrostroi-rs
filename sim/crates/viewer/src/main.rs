mod app;
mod audio;
mod car;
mod config;
mod dynamics;
mod ground;
mod menu;
mod panels;
mod renderer;
mod signals;
mod station;
mod sim;
mod track;
mod ui;

use anyhow::{Context, Result};
use formats::vfs::Vfs;
use glam::Mat4;
use std::path::{Path, PathBuf};

pub fn mount_workshop(cfg: &config::Config) -> Result<Vfs> {
    let mut vfs = Vfs::default();
    for id in &cfg.packs {
        let dir = Path::new(&cfg.workshop).join(id);
        let gma = std::fs::read_dir(&dir)
            .with_context(|| format!("workshop item {id} not found in {}", cfg.workshop))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|x| x == "gma" || x == "bin"))
            .with_context(|| format!("no .gma in {}", dir.display()))?;
        vfs.mount(&gma)?;
    }
    // base-game content (HL2/CSS textures the map uses): <steam>/steamapps/common/GarrysMod/{garrysmod,sourceengine}
    let gmod = Path::new(&cfg.workshop).ancestors().nth(3).map(|a| a.join("common/GarrysMod"));
    let mut mounted = 0;
    for sub in ["garrysmod", "sourceengine"] {
        let Some(dir) = gmod.as_ref().map(|g| g.join(sub)) else { continue };
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        let mut files: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.to_string_lossy().ends_with("_dir.vpk")).collect();
        files.sort();
        for f in files {
            match vfs.mount_vpk(&f) {
                Ok(()) => mounted += 1,
                Err(e) => log::warn!("vpk {}: {e:#}", f.display()),
            }
        }
    }
    log::info!("mounted {mounted} base-game packages");
    Ok(vfs)
}

/// The map's `skyname` (worldspawn key).
fn sky_name(bsp: &[u8]) -> Option<String> {
    formats::bsp::entities(bsp).ok()?.into_iter().find_map(|e| e.get("skyname").cloned())
}

/// A cube of six skybox faces (Source space, half-size 5000 units) with the `<sky><face>` materials.
fn sky_meshes(name: &str) -> Vec<formats::mdl::Mesh> {
    const S: f32 = 5000.0;
    // face suffix, four corners (top-left, top-right, bottom-right, bottom-left as seen from inside)
    let faces: [(&str, [[f32; 3]; 4]); 6] = [
        ("ft", [[S, S, S], [S, -S, S], [S, -S, -S], [S, S, -S]]),
        ("bk", [[-S, -S, S], [-S, S, S], [-S, S, -S], [-S, -S, -S]]),
        ("lf", [[-S, S, S], [S, S, S], [S, S, -S], [-S, S, -S]]),
        ("rt", [[S, -S, S], [-S, -S, S], [-S, -S, -S], [S, -S, -S]]),
        ("up", [[S, -S, S], [S, S, S], [-S, S, S], [-S, -S, S]]),
        ("dn", [[S, S, -S], [S, -S, -S], [-S, -S, -S], [-S, S, -S]]),
    ];
    faces
        .iter()
        .map(|(suffix, c)| {
            let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
            let vertices = (0..4)
                .map(|i| formats::mdl::Vertex { pos: c[i], normal: [0.0, 0.0, 1.0], uv: uv[i], ..Default::default() })
                .collect();
            formats::mdl::Mesh { material: format!("skybox/{name}{suffix}"), vertices, indices: vec![0, 1, 2, 0, 2, 3, 0, 2, 1, 0, 3, 2] }
        })
        .collect()
}

fn arg_value(args: &[String], key: &str) -> Option<String> {
    args.iter().position(|a| a == key).and_then(|i| args.get(i + 1)).cloned()
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,wgpu=warn,naga=warn")).init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let shot: Option<PathBuf> = arg_value(&args, "--shot").map(PathBuf::from);
    let yaw: f32 = arg_value(&args, "--yaw").and_then(|s| s.parse().ok()).unwrap_or(-0.8);
    let pitch: f32 = arg_value(&args, "--pitch").and_then(|s| s.parse().ok()).unwrap_or(0.35);
    let zoom: f32 = arg_value(&args, "--zoom").and_then(|s| s.parse().ok()).unwrap_or(2.4);
    // Optional: look at a point (metres, car space) instead of the car's centre.
    let look: Option<Vec<f32>> = arg_value(&args, "--look")
        .map(|s| s.split(',').filter_map(|v| v.parse().ok()).collect());
    let single = arg_value(&args, "--model");
    let (cfg, game) = config::Config::load();
    log::info!("game folder: {:?}", game);
    let controls = arg_value(&args, "--controls").unwrap_or_else(|| cfg.resolve(game.as_deref(), &cfg.controls).to_string_lossy().into_owned());

    let vfs = mount_workshop(&cfg)?;
    log::info!("mounted {} files", vfs.len());

    let map_dir = if cfg.map.is_empty() { None } else { Some(cfg.resolve(game.as_deref(), &cfg.map)) };
    let at: Option<f32> = arg_value(&args, "--at").and_then(|s| s.parse().ok());
    let lua_root = cfg.resolve(game.as_deref(), &cfg.lua_root).to_string_lossy().into_owned();
    let train = arg_value(&args, "--train").unwrap_or_else(|| cfg.train.clone());
    let mut obstacles = cfg.obstacles.clone();
    if let Some(b) = arg_value(&args, "--block").and_then(|s| s.parse().ok()) {
        obstacles.push(b);
    }
    let map_ents = map_dir
        .as_ref()
        .and_then(|d| std::fs::read(d.join("map.bsp")).ok())
        .and_then(|b| formats::bsp::entities(&b).ok())
        .unwrap_or_default();
    let lead: Vec<f32> = arg_value(&args, "--lead").map(|v| v.split(',').filter_map(|x| x.parse().ok()).collect()).unwrap_or_default();
    let make_sim = move || -> Result<sim::Sim> {
        let sim = sim::Sim::new(&train, &lua_root).context("train simulation")?;
        if let Some(dir) = &map_dir {
            match track::Track::load(&dir.join("track.lua")) {
                Ok(t) => {
                    if let Ok(s) = track::load_signs(&dir.join("signs.lua"), t.main()) {
                        sim.set_signs(s);
                    }
                    sim.set_platforms(station::load(&map_ents, t.main()));
                    sim.set_station_names(station::names(&std::path::Path::new(&lua_root).join("metrostroi/maps/crossline.lua")));
                    sim.set_track(t);
                }
                Err(e) => log::warn!("no track: {e:#}"),
            }
        }
        if let Some(s) = at {
            sim.set_position(s);
        }
        sim.set_obstacles(obstacles.clone());
        sim.set_leads(lead.clone());
        Ok(sim)
    };
    let sim = make_sim()?;
    let props = sim.props();
    let body_model = sim.body_model().unwrap_or_else(|| "models/metrostroi_train/81-717/81-717_mvm".into());
    log::info!("body {body_model}");
    let skin = sim.skin_map();
    log::info!("{} skin texture replacements", skin.len());
    log::info!("{} client props", props.len());

    if shot.is_none() {
        let controls_path = PathBuf::from(&controls);
        let start_cab = args.iter().any(|a| a == "--cab");
        let no_props = args.iter().any(|a| a == "--noprops");
        let lightmaps = !args.iter().any(|a| a == "--nolightmap");
        let map_bsp = cfg.resolve(game.as_deref(), &cfg.map).join("map.bsp");
        let with_world = !cfg.map.is_empty() && map_bsp.exists() && !args.iter().any(|a| a == "--noworld");
        let vfs = std::sync::Arc::new(std::sync::Mutex::new(vfs));
        let audio = audio::Audio::new(vfs.clone());
        if audio.is_none() {
            log::warn!("no audio device");
        }
        let signs = sim.signs();
        let mut standing: Vec<Mat4> = sim.obstacles().iter().filter_map(|&s| sim.standing_pose(s)).collect();
        let leads = sim.lead_poses();
        let n_lead = leads.len();
        standing.extend(leads);
        let lua_root = PathBuf::from(cfg.resolve(game.as_deref(), &cfg.lua_root));
        let tips = controls_path.parent().map(|d| d.join("tooltips_ru.json"));
        let panels = match sim.panels(tips.as_deref()) {
            Some(p) => p,
            None => panels::Panels::load(&controls_path)?,
        };
        log::info!("{} buttons on panels", panels.button_count());
        return app::run(
            Box::new(move |r| match single {
                Some(m) => {
                    let mut s = renderer::Scene::default();
                    let id = r.load_model(&mut s, &vfs.lock().unwrap(), &m)?;
                    r.add_object(&mut s, "model", id, Mat4::IDENTITY, true);
                    Ok(s)
                }
                None => {
                    let mut vfs = vfs.lock().unwrap();
                    *r.skin.borrow_mut() = skin;
                    let mut scene = car::load(r, &vfs, &body_model, &props)?;
                    if with_world {
                        let data = std::fs::read(&map_bsp)?;
                        let (mut meshes, lightmap) = formats::bsp::load_lit(&data)?;
                        vfs.mount_files(formats::bsp::pakfile(&data)?);
                        ground::store(ground::Ground::build(&meshes, renderer::SCALE));
                        formats::bsp::double_sided(&mut meshes);
                        let id = r.load_meshes(&mut scene, &vfs, "world", &meshes, renderer::SCALE);
                        r.add_object(&mut scene, "world", id, Mat4::IDENTITY, true);
                        if let Some(sky) = sky_name(&data) {
                            let id = r.load_meshes(&mut scene, &vfs, "sky", &sky_meshes(&sky), renderer::SCALE);
                            r.add_object(&mut scene, "sky", id, Mat4::IDENTITY, false);
                            scene.objects.last_mut().unwrap().tint = [1.0, 1.0, 1.0, 4.0];
                        }
                        if let (Some(lm), true) = (&lightmap, lightmaps) {
                            r.set_lightmap(lm);
                            scene.objects.last_mut().unwrap().tint[3] = 3.0;
                        }
                        let statics = if no_props { Vec::new() } else { formats::bsp::static_props(&data).unwrap_or_default() };
                        signals::store(signals::Signals::spawn(r, &mut scene, &vfs, &lua_root, signs, standing, n_lead).with_statics(r, &mut scene, &vfs, &statics));
                    }
                    Ok(scene)
                }
            }),
            panels,
            Some(sim),
            Box::new(make_sim),
            audio,
            start_cab,
            game.clone().unwrap_or_default().join("config").join("settings.json"),
            !args.iter().any(|a| a == "--nomenu"),
        );
    }

    pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() })
            .await?;
        log::info!("adapter: {}", adapter.get_info().name);
        let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await?;
        let r = renderer::Renderer::new(device, queue, renderer::COLOR_FORMAT);
        r.unlit.set(args.iter().any(|a| a == "--unlit"));

        *r.skin.borrow_mut() = skin;
        let t = std::time::Instant::now();
        let scene = match single {
            Some(m) => {
                let mut s = renderer::Scene::default();
                let id = r.load_model(&mut s, &vfs, &m)?;
                r.add_object(&mut s, "model", id, Mat4::IDENTITY, true);
                s
            }
            None => car::load(&r, &vfs, &body_model, &props)?,
        };
        let mut scene = scene;
        car::reset_poses(&r, &mut scene);
        let names = car::index(&scene);
        for _ in 0..400 {
            sim.tick(1.0 / 60.0);
        }
        // optional switch presses before the shot: --press A1Toggle,VBToggle
        if let Some(list) = arg_value(&args, "--press") {
            for id in list.split(',') {
                log::info!("press {id} -> {:?}", sim.press(id));
            }
            for _ in 0..120 {
                sim.tick(1.0 / 60.0);
            }
        }
        car::apply_state(&r, &mut scene, &names, &sim.state());
        log::info!("scene built in {:?}", t.elapsed());
        if let Some(list) = arg_value(&args, "--pose") {
            for kv in list.split(',') {
                let Some((name, v)) = kv.split_once('=') else { continue };
                let v: f32 = v.parse().unwrap_or(0.0);
                match scene.objects.iter().position(|o| o.name == name) {
                    Some(i) => {
                        if !r.set_pose_param(&mut scene, i, "position", v) {
                            log::warn!("{name}: no pose parameter 'position'");
                        }
                    }
                    None => log::warn!("no object {name}"),
                }
            }
        }
        if args.iter().any(|a| a == "--dump") {
            for o in &scene.objects {
                let (a, b) = scene.object_bounds(o);
                println!("{:<22} vis={} x {:6.2}..{:6.2}  y {:6.2}..{:6.2}  z {:6.2}..{:6.2}", o.name, o.visible as u8, a.x, b.x, a.y, b.y, a.z, b.z);
            }
        }
        // --world <s>: add the map's BSP and look down the track from `s` metres along the main line
        let mut world_cam: Option<(glam::Vec3, glam::Vec3)> = None;
        if let Some(sv) = arg_value(&args, "--world") {
            let s: f32 = sv.parse().unwrap_or(0.0);
            let bsp = cfg.resolve(game.as_deref(), &cfg.map).join("map.bsp");
            let data = std::fs::read(&bsp).with_context(|| format!("{} (extract the map first)", bsp.display()))?;
            let (mut meshes, lightmap) = formats::bsp::load_lit(&data)?;
            formats::bsp::double_sided(&mut meshes);
            let mut vfs = vfs;
            vfs.mount_files(formats::bsp::pakfile(&data)?);
            let id = r.load_meshes(&mut scene, &vfs, "world", &meshes, renderer::SCALE);
            r.add_object(&mut scene, "world", id, Mat4::IDENTITY, true);
            if let Some(sky) = sky_name(&data) {
                let id = r.load_meshes(&mut scene, &vfs, "sky", &sky_meshes(&sky), renderer::SCALE);
                r.add_object(&mut scene, "sky", id, Mat4::IDENTITY, true);
                scene.objects.last_mut().unwrap().tint = [1.0, 1.0, 1.0, 4.0];
            }
            if let (Some(lm), true) = (&lightmap, !args.iter().any(|a| a == "--nolightmap")) {
                r.set_lightmap(lm);
                scene.objects.last_mut().unwrap().tint[3] = 3.0;
            }
            if !args.iter().any(|a| a == "--noprops") {
                // static props near the camera spot, for a quick look (the live view streams them by distance)
                let tr0 = track::Track::load(&cfg.resolve(game.as_deref(), &cfg.map).join("track.lua"))?;
                let c = tr0.main().at(s).pos;
                let near: Vec<_> = formats::bsp::static_props(&data)?
                    .into_iter()
                    .filter(|p| (glam::Vec3::from(p.origin) * renderer::SCALE - c).length() < 250.0)
                    .collect();
                for p in &near {
                    if let Ok(id) = r.load_model(&mut scene, &vfs, p.model.trim_end_matches(".mdl")) {
                        let t = Mat4::from_translation(glam::Vec3::from(p.origin) * renderer::SCALE) * renderer::source_angles(p.angles[0], p.angles[1], p.angles[2]);
                        r.add_object(&mut scene, "static prop", id, t, true);
                        let a = p.ambient.map(|c| (c * 7.0).clamp(0.35, 1.6));
                        scene.objects.last_mut().unwrap().tint = [a[0], a[1], a[2], 5.0];
                    }
                }
                log::info!("{} static props near the spot", near.len());
            }
            let tr = track::Track::load(&cfg.resolve(game.as_deref(), &cfg.map).join("track.lua"))?;
            let p = tr.main().at(s);
            // track points are in metres at Metrostroi's scale; the world is drawn at SCALE
            world_cam = Some((p.pos + glam::Vec3::Z * 2.0, p.dir));
        }
        let (mn, mx) = scene.bounds();
        let center = look.as_ref().filter(|l| l.len() == 3).map_or((mn + mx) * 0.5, |l| glam::Vec3::new(l[0], l[1], l[2]));
        let radius = (mx - mn).length() * 0.5;
        log::info!("bounds {mn:?} .. {mx:?}");
        let (w, h) = (1600u32, 900u32);
        let dist = if look.is_some() { zoom } else { radius * zoom };
        let vp = if let Some((eye, dir)) = world_cam {
            Mat4::perspective_rh(65f32.to_radians(), w as f32 / h as f32, 0.1, 2000.0) * Mat4::look_to_rh(eye, dir, glam::Vec3::Z)
        } else {
            renderer::orbit_view_proj(center, if look.is_some() { zoom } else { radius }, yaw, pitch, dist, w as f32 / h as f32)
        };
        match shot {
            Some(path) => {
                r.screenshot(&scene, vp, w, h, &path)?;
                log::info!("wrote {}", path.display());
                Ok(())
            }
            None => unreachable!("interactive mode returned earlier"),
        }
    })
}
