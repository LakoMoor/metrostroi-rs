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
    Ok(vfs)
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
    let train = cfg.train.clone();
    let mut obstacles = cfg.obstacles.clone();
    if let Some(b) = arg_value(&args, "--block").and_then(|s| s.parse().ok()) {
        obstacles.push(b);
    }
    let map_ents = map_dir
        .as_ref()
        .and_then(|d| std::fs::read(d.join("map.bsp")).ok())
        .and_then(|b| formats::bsp::entities(&b).ok())
        .unwrap_or_default();
    let lead: Option<f32> = arg_value(&args, "--lead").and_then(|s| s.parse().ok());
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
        if let Some(s) = lead {
            sim.set_lead(s);
        }
        Ok(sim)
    };
    let sim = make_sim()?;
    let props = sim.props();
    let skin = sim.skin_map();
    log::info!("{} skin texture replacements", skin.len());
    log::info!("{} client props", props.len());

    if shot.is_none() {
        let controls_path = PathBuf::from(&controls);
        let start_cab = args.iter().any(|a| a == "--cab");
        let map_bsp = cfg.resolve(game.as_deref(), &cfg.map).join("map.bsp");
        let with_world = !cfg.map.is_empty() && map_bsp.exists() && !args.iter().any(|a| a == "--noworld");
        let vfs = std::sync::Arc::new(std::sync::Mutex::new(vfs));
        let audio = audio::Audio::new(vfs.clone());
        if audio.is_none() {
            log::warn!("no audio device");
        }
        let signs = sim.signs();
        let mut standing: Vec<Mat4> = sim.obstacles().iter().filter_map(|&s| sim.standing_pose(s)).collect();
        let has_lead = sim.lead_pose().is_some();
        if let Some(p) = sim.lead_pose() {
            standing.push(p);
        }
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
                    let mut scene = car::load(r, &vfs, "models/metrostroi_train/81-717/81-717_mvm", &props)?;
                    if with_world {
                        let data = std::fs::read(&map_bsp)?;
                        let meshes = formats::bsp::load(&data)?;
                        vfs.mount_files(formats::bsp::pakfile(&data)?);
                        ground::store(ground::Ground::build(&meshes, renderer::SCALE));
                        let id = r.load_meshes(&mut scene, &vfs, "world", &meshes, renderer::SCALE);
                        r.add_object(&mut scene, "world", id, Mat4::IDENTITY, true);
                        signals::store(signals::Signals::spawn(r, &mut scene, &vfs, &lua_root, signs, standing, has_lead));
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
            None => car::load(&r, &vfs, "models/metrostroi_train/81-717/81-717_mvm", &props)?,
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
            let meshes = formats::bsp::load(&data)?;
            let mut vfs = vfs;
            vfs.mount_files(formats::bsp::pakfile(&data)?);
            let id = r.load_meshes(&mut scene, &vfs, "world", &meshes, renderer::SCALE);
            r.add_object(&mut scene, "world", id, Mat4::IDENTITY, true);
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
