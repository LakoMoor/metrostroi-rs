//! Assemble a car from its entity body model and the client props its own Lua defines.

use crate::renderer::{source_angles, Renderer, Scene, SCALE};
use crate::sim::PropDef;
use anyhow::Result;
use formats::vfs::Vfs;
use glam::{Mat4, Vec3};
use std::collections::HashMap;

/// (name, x, yaw) of the 81-717's bogeys in car space (Source units / degrees).
const BOGEYS: [(&str, f32, f32); 2] = [("bogey_front", 312.0, 180.0), ("bogey_rear", -317.0, 0.0)];
/// Wheel radius, metres (the wheelset model is 0.86 m tall).
const WHEEL_R: f32 = 0.43;

fn bogey_base(x: f32, yaw: f32) -> Mat4 {
    Mat4::from_translation(Vec3::new(x, 0.0, -84.0) * SCALE) * source_angles(0.0, yaw, 0.0)
}

fn wheels_base(x: f32, yaw: f32) -> Mat4 {
    bogey_base(x, yaw) * Mat4::from_translation(Vec3::new(0.0, 0.0, -10.0) * SCALE) * source_angles(0.0, 90.0, 0.0)
}

/// Turn the wheelsets to match `travelled` metres of rolling (the axle is the model's local X; the front
/// bogey is turned half way round, so it spins the other way).
pub fn spin_wheels(scene: &mut Scene, names: &HashMap<String, usize>, travelled: f64) {
    let turn = (travelled / WHEEL_R as f64).rem_euclid(std::f64::consts::TAU) as f32;
    for (name, x, yaw) in BOGEYS {
        let Some(&i) = names.get(&format!("{name}_wheels")) else { continue };
        let sign = if yaw > 90.0 { -1.0 } else { 1.0 };
        scene.objects[i].transform = wheels_base(x, yaw) * Mat4::from_rotation_x(sign * turn);
    }
}

pub fn load(r: &Renderer, vfs: &Vfs, body_model: &str, props: &[PropDef]) -> Result<Scene> {
    let mut scene = Scene::default();
    let body = r.load_model(&mut scene, vfs, body_model)?;
    r.add_object(&mut scene, "body", body, Mat4::IDENTITY, true);

    // bogeys are separate entities in Metrostroi (gmod_train_bogey); positions are the 81-717's CreateBogey calls
    for (name, x, yaw) in BOGEYS {
        let at = bogey_base(x, yaw);
        if let Ok(id) = r.load_model(&mut scene, vfs, "models/metrostroi_train/bogey/metro_bogey_717") {
            r.add_object(&mut scene, name, id, at, true);
        }
        if let Ok(id) = r.load_model(&mut scene, vfs, "models/metrostroi_train/bogey/metro_wheels_collector") {
            let w = wheels_base(x, yaw);
            r.add_object(&mut scene, &format!("{name}_wheels"), id, w, true);
        }
    }

    let mut skipped = 0;
    for p in props {
        let id = match r.load_model(&mut scene, vfs, &p.model) {
            Ok(id) => id,
            Err(e) => {
                log::warn!("prop {}: {e}", p.name);
                skipped += 1;
                continue;
            }
        };
        let t = Mat4::from_translation(Vec3::from(p.pos) * SCALE) * source_angles(p.ang[0], p.ang[1], p.ang[2]);
        // hidden until the cab's Lua reports it shown
        r.add_object(&mut scene, &p.name, id, t, false);
    }
    log::info!("car: {} objects, {} skipped", scene.objects.len(), skipped);
    Ok(scene)
}

/// Object index by name.
pub fn index(scene: &Scene) -> HashMap<String, usize> {
    scene.objects.iter().enumerate().map(|(i, o)| (o.name.clone(), i)).collect()
}

/// Show exactly the props the cab's Lua reports, with their pose-parameter values.
pub fn apply_state(r: &Renderer, scene: &mut Scene, names: &HashMap<String, usize>, state: &HashMap<String, (Option<f32>, f32)>, light: f32) {
    for (name, &i) in names {
        if name == "world" {
            continue;
        }
        if name.starts_with("bogey") {
            scene.objects[i].tint = [light, light, light, 1.0];
            continue;
        }
        if name == "body" || name == "model" {
            scene.objects[i].tint = [light, light, light, 1.0];
            continue;
        }
        let shown = state.get(name).filter(|s| s.1 > 0.02);
        scene.objects[i].visible = shown.is_some();
        if let Some(&(_, a)) = shown {
            let lamp = {
                let n = name.to_ascii_lowercase();
                n.contains("lamp") || n.contains("light")
            };
            // lamps shine on their own; everything else follows the light around the car
            let a = if lamp { a } else { a * light };
            scene.objects[i].tint = [a, a, a, if lamp { 2.0 } else { 1.0 }];
        }
    }
    for (name, (pose, _)) in state {
        // a prop without a recorded pose is at the default 0 (a re-created door model starts closed)
        let Some(&i) = names.get(name) else { continue };
        let v = pose.unwrap_or(0.0);
        if scene.last_pose.get(&i).is_some_and(|&p| (p - v).abs() < 1e-4) {
            continue;
        }
        r.set_pose_param(scene, i, "position", v);
        scene.last_pose.insert(i, v);
    }
}

/// Props are modelled in their open/bind pose; put every one with a `position` pose parameter at 0
/// (closed doors, released switches) until the cab code drives it.
pub fn reset_poses(r: &Renderer, scene: &mut Scene) {
    for i in 0..scene.objects.len() {
        r.set_pose_param(scene, i, "position", 0.0);
        scene.last_pose.insert(i, 0.0);
    }
}
