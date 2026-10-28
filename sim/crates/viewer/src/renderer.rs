use anyhow::Result;
use formats::{mdl, vfs::Vfs, vmt, vtf};
use glam::{Mat4, Vec3};
use std::collections::HashMap;
use wgpu::util::DeviceExt;

pub const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Source inch -> metre.
/// Metres per Source unit: Metrostroi's own scale (a 717 body is 18.1 m long, not 24 m).
pub const SCALE: f32 = 0.01905;
/// Dynamic-offset stride of the per-object uniform (must satisfy the device alignment).
const OBJ_STRIDE: u64 = 256;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuVertex {
    pos: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
    uv2: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    light: [f32; 4],
    /// x = tone-mapping strength (0 = off), y = saturation boost
    fx: [f32; 4],
}

#[derive(Clone, Copy, PartialEq)]
enum Blend {
    Opaque,
    AlphaTest,
    Translucent,
}

struct MaterialGpu {
    bind: wgpu::BindGroup,
    blend: Blend,
}

struct DrawMesh {
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    n_indices: u32,
    material: usize,
}

struct GpuModel {
    meshes: Vec<DrawMesh>,
    /// Bind-pose vertices (Source inches) per mesh, kept only for models with a skeleton.
    skin_src: Vec<Vec<mdl::Vertex>>,
    skeleton: Option<formats::skeleton::Skeleton>,
    /// Bounds in model space, metres.
    min: Vec3,
    max: Vec3,
}

pub struct Object {
    pub name: String,
    model: usize,
    /// Model space (Source axes, metres) -> car space.
    pub transform: Mat4,
    pub visible: bool,
    /// Colour multiplier (lamp glow brightness etc.).
    pub tint: [f32; 4],
    /// Per-object posed copy of the model's vertices (pose-parameter models only).
    skin: Option<Skin>,
}

struct Skin {
    vbufs: Vec<wgpu::Buffer>,
    params: Vec<f32>,
}

#[derive(Default)]
pub struct Scene {
    models: Vec<GpuModel>,
    model_ids: HashMap<String, usize>,
    materials: Vec<MaterialGpu>,
    material_ids: HashMap<String, usize>,
    pub objects: Vec<Object>,
    /// Last `position` pose applied per object, so unchanged props are not re-skinned every frame.
    pub last_pose: HashMap<usize, f32>,
}

impl Scene {
    /// World-space bounds of one object.
    pub fn object_bounds(&self, o: &Object) -> (Vec3, Vec3) {
        let m = &self.models[o.model];
        let (mut mn, mut mx) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for i in 0..8 {
            let c = Vec3::new(
                if i & 1 == 0 { m.min.x } else { m.max.x },
                if i & 2 == 0 { m.min.y } else { m.max.y },
                if i & 4 == 0 { m.min.z } else { m.max.z },
            );
            let w = o.transform.transform_point3(c);
            mn = mn.min(w);
            mx = mx.max(w);
        }
        (mn, mx)
    }

    /// World-space bounds of all visible objects (by their model bounds), the map excluded.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let (mut mn, mut mx) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for o in self.objects.iter().filter(|o| o.visible && o.name != "world") {
            let m = &self.models[o.model];
            for i in 0..8 {
                let c = Vec3::new(
                    if i & 1 == 0 { m.min.x } else { m.max.x },
                    if i & 2 == 0 { m.min.y } else { m.max.y },
                    if i & 4 == 0 { m.min.z } else { m.max.z },
                );
                let w = o.transform.transform_point3(c);
                mn = mn.min(w);
                mx = mx.max(w);
            }
        }
        (mn, mx)
    }
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    globals: wgpu::Buffer,
    globals_bind: std::cell::RefCell<wgpu::BindGroup>,
    globals_layout: wgpu::BindGroupLayout,
    lm_sampler: wgpu::Sampler,
    material_layout: wgpu::BindGroupLayout,
    object_layout: wgpu::BindGroupLayout,
    object_buf: std::cell::RefCell<(wgpu::Buffer, u64)>,
    object_bind: std::cell::RefCell<wgpu::BindGroup>,
    sampler: wgpu::Sampler,
    /// Pipelines per sample count: index 0 = 1x, index 1 = 4x MSAA.
    pipes: [[wgpu::RenderPipeline; 3]; 3],
    /// Draw into the HDR format instead of the swap-chain format (ultra tier).
    pub hdr: std::cell::Cell<bool>,
    /// Current MSAA sample count (1 or 4); `depth_view` and `msaa_view` follow it.
    pub samples: std::cell::Cell<u32>,
    /// Post-tone-mapping amount for the ultra quality tier (0..1).
    pub tonemap: std::cell::Cell<f32>,
    pub target_format: wgpu::TextureFormat,
    /// Debug: show raw texture colours without lighting.
    pub unlit: std::cell::Cell<bool>,
    /// Skin material replacements: texture basename -> material path.
    pub skin: std::cell::RefCell<HashMap<String, String>>,
}

pub fn orbit_view_proj(center: Vec3, radius: f32, yaw: f32, pitch: f32, dist: f32, aspect: f32) -> Mat4 {
    // Z is up (Source convention); yaw spins around Z.
    let dir = Vec3::new(yaw.cos() * pitch.cos(), yaw.sin() * pitch.cos(), pitch.sin());
    let eye = center + dir * dist;
    let view = Mat4::look_at_rh(eye, center, Vec3::Z);
    let proj = Mat4::perspective_rh(45f32.to_radians(), aspect, (radius * 0.005).max(0.02), radius * 20.0);
    proj * view
}

/// Source `Angle(pitch, yaw, roll)` in degrees to a rotation matrix (X forward, Y left, Z up).
pub fn source_angles(pitch: f32, yaw: f32, roll: f32) -> Mat4 {
    Mat4::from_rotation_z(yaw.to_radians())
        * Mat4::from_rotation_y(pitch.to_radians())
        * Mat4::from_rotation_x(roll.to_radians())
}

impl Renderer {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_entry = |dynamic: bool, size: Option<u64>| wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: dynamic,
                min_binding_size: size.and_then(std::num::NonZeroU64::new),
            },
            count: None,
        };
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals layout"),
            entries: &[
                uniform_entry(false, None),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let object_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("object layout"),
            entries: &[uniform_entry(true, Some(80))],
        });
        let lm_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let white = Self::lightmap_view(&device, &queue, 1, 1, &[128, 128, 128, 255]);
        let globals_bind = Self::make_globals_bind(&device, &globals_layout, &globals, &white, &lm_sampler);
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("material layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 8,
            ..Default::default()
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline layout"),
            bind_group_layouts: &[Some(&globals_layout), Some(&material_layout), Some(&object_layout)],
            immediate_size: 0,
        });
        let make = |label: &str, fs: &str, blend: Option<wgpu::BlendState>, depth_write: bool, samples: u32, target_format: wgpu::TextureFormat| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<GpuVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x2],
                    })],
                },
                primitive: wgpu::PrimitiveState { cull_mode: if std::env::var_os("MS_NOCULL").is_some() { None } else { Some(wgpu::Face::Back) }, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let set = |n: u32, f: wgpu::TextureFormat| {
            [
                make("opaque", "fs_opaque", None, true, n, f),
                make("alphatest", "fs_alphatest", None, true, n, f),
                make("blend", "fs_blend", Some(wgpu::BlendState::ALPHA_BLENDING), false, n, f),
            ]
        };
        let pipes = [set(1, target_format), set(4, target_format), set(4, crate::post::HDR_FORMAT)];
        let (obuf, obind) = Self::make_object_buffer(&device, &object_layout, 1);
        Renderer {
            device,
            queue,
            globals,
            globals_bind: std::cell::RefCell::new(globals_bind),
            globals_layout,
            lm_sampler,
            material_layout,
            object_layout,
            object_buf: std::cell::RefCell::new((obuf, 1)),
            object_bind: std::cell::RefCell::new(obind),
            sampler,
            pipes,
            hdr: std::cell::Cell::new(false),
            samples: std::cell::Cell::new(1),
            tonemap: std::cell::Cell::new(0.0),
            target_format,
            unlit: std::cell::Cell::new(false),
            skin: Default::default(),
        }
    }

    fn lightmap_view(device: &wgpu::Device, queue: &wgpu::Queue, w: u32, h: u32, rgba: &[u8]) -> wgpu::TextureView {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("lightmap"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        tex.create_view(&Default::default())
    }

    fn make_globals_bind(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, globals: &wgpu::Buffer, lm: &wgpu::TextureView, smp: &wgpu::Sampler) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals bind"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(lm) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(smp) },
            ],
        })
    }

    /// Use the map's lightmap atlas for objects tinted with alpha 3 (see the shader).
    pub fn set_lightmap(&self, lm: &formats::bsp::Lightmap) {
        let view = Self::lightmap_view(&self.device, &self.queue, lm.width, lm.height, &lm.rgba);
        *self.globals_bind.borrow_mut() = Self::make_globals_bind(&self.device, &self.globals_layout, &self.globals, &view, &self.lm_sampler);
    }

    fn make_object_buffer(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, n: u64) -> (wgpu::Buffer, wgpu::BindGroup) {
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("objects"),
            size: n * OBJ_STRIDE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("objects bind"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &buf, offset: 0, size: std::num::NonZeroU64::new(80) }),
            }],
        });
        (buf, bind)
    }

    fn upload_texture(&self, v: &vtf::Vtf, label: &str) -> wgpu::TextureView {
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: v.width, height: v.height, depth_or_array_layers: 1 },
            mip_level_count: v.mips.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (m, data) in v.mips.iter().enumerate() {
            let (w, h) = ((v.width >> m).max(1), (v.height >> m).max(1));
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: m as u32, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                data,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
        }
        tex.create_view(&Default::default())
    }

    fn material(&self, vfs: &Vfs, cds: &[String], name: &str) -> MaterialGpu {
        let lname = name.to_ascii_lowercase();
        let mut found = None;
        for cd in cds.iter().map(|s| s.as_str()).chain(std::iter::once("")) {
            if let Some(b) = vfs.read(&format!("materials/{cd}{lname}.vmt")) {
                found = Some(vmt::parse(&String::from_utf8_lossy(b)));
                break;
            }
        }
        let mut blend = Blend::Opaque;
        let mut view = None;
        match &found {
            Some(m) => {
                if m.flag("$translucent") {
                    blend = Blend::Translucent;
                } else if m.flag("$alphatest") {
                    blend = Blend::AlphaTest;
                }
                if m.base_texture().is_none() {
                    log::warn!("material {name}: no base texture (shader {})", m.shader);
                }
                if let Some(t) = m.base_texture() {
                    let p = format!("materials/{}.vtf", t.replace('\\', "/"));
                    match vfs.read(&p).map(vtf::decode) {
                        Some(Ok(v)) => {
                            let px = &v.mips[0];
                            let lum: f32 = px.chunks_exact(4).map(|c| (c[0] as f32 + c[1] as f32 + c[2] as f32) / 765.0).sum::<f32>() / (px.len() / 4).max(1) as f32;
                            if std::env::var_os("MS_MATLOG").is_some() {
                                let n = (px.len() / 4).max(1) as f32;
                                let avg: Vec<f32> = (0..3).map(|k| px.chunks_exact(4).map(|c| c[k] as f32).sum::<f32>() / n).collect();
                                eprintln!("MAT {name} {p} fmt avg {:?}", avg);
                            }
                            if lum < 0.04 {
                                log::warn!("dark material {name} -> {p} (avg {lum:.3}, shader {})", m.shader);
                            }
                            view = Some(self.upload_texture(&v, &p))
                        }
                        Some(Err(e)) => log::warn!("{p}: {e}"),
                        None => log::warn!("missing texture {p}"),
                    }
                }
            }
            None => log::warn!("missing material {name}"),
        }
        let view = view.unwrap_or_else(|| {
            // models keep the loud magenta; map surfaces (no cd path) get a muted stand-in guessed from the name
            let px = if cds.is_empty() { placeholder_colour(&lname) } else { [255, 0, 255, 255] };
            self.upload_texture(&vtf::Vtf { width: 1, height: 1, mips: vec![px.to_vec()], has_alpha: false }, "missing")
        });
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(name),
            layout: &self.material_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        MaterialGpu { bind, blend }
    }

    /// Load (or reuse) a model by path without extension. Uses bodypart model 0 of every bodypart.
    pub fn load_model(&self, scene: &mut Scene, vfs: &Vfs, path: &str) -> Result<usize> {
        let key = path.to_ascii_lowercase();
        if let Some(&id) = scene.model_ids.get(&key) {
            return Ok(id);
        }
        let get = |ext: &str| {
            vfs.read(&format!("{key}.{ext}")).ok_or_else(|| anyhow::anyhow!("{key}.{ext} not found"))
        };
        let model = mdl::load(get("mdl")?, get("vvd")?, get("dx90.vtx")?)?;
        let mut meshes = Vec::new();
        let mut skin_src = Vec::new();
        let (mut mn, mut mx) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for bp in &model.body_parts {
            let Some(sm) = bp.models.first() else { continue };
            for me in &sm.meshes {
                let verts: Vec<GpuVertex> = me
                    .vertices
                    .iter()
                    .map(|v| GpuVertex {
                        pos: [v.pos[0] * SCALE, v.pos[1] * SCALE, v.pos[2] * SCALE],
                        normal: v.normal,
                        uv: v.uv,
                        uv2: [0.0; 2],
                    })
                    .collect();
                for v in &verts {
                    let p = Vec3::from(v.pos);
                    mn = mn.min(p);
                    mx = mx.max(p);
                }
                let base = me.material.rsplit(|c| c == '/' || c == '\\').next().unwrap_or("").to_ascii_lowercase();
                let replaced = self.skin.borrow().get(&base).cloned().filter(|r| {
                    let ok = vfs.exists(&format!("materials/{}.vmt", r.to_ascii_lowercase().replace('\\', "/")));
                    if !ok {
                        log::debug!("skin material {r} not found, keeping {}", me.material);
                    }
                    ok
                });
                let (cds, mname): (&[String], &str) = match &replaced {
                    Some(r) => (&[], r.as_str()),
                    None => (&model.cd_textures, me.material.as_str()),
                };
                let mkey = format!("{}|{}", cds.first().map(|s| s.as_str()).unwrap_or(""), mname.to_ascii_lowercase());
                let mat = match scene.material_ids.get(&mkey) {
                    Some(&i) => i,
                    None => {
                        scene.materials.push(self.material(vfs, cds, mname));
                        scene.material_ids.insert(mkey, scene.materials.len() - 1);
                        scene.materials.len() - 1
                    }
                };
                if model.skeleton.is_some() {
                    skin_src.push(me.vertices.clone());
                }
                meshes.push(DrawMesh {
                    vbuf: self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("vb"),
                        contents: bytemuck::cast_slice(&verts),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
                    ibuf: self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("ib"),
                        contents: bytemuck::cast_slice(&me.indices),
                        usage: wgpu::BufferUsages::INDEX,
                    }),
                    n_indices: me.indices.len() as u32,
                    material: mat,
                });
            }
        }
        if meshes.is_empty() {
            mn = Vec3::ZERO;
            mx = Vec3::ZERO;
        }
        scene.models.push(GpuModel { meshes, skin_src, skeleton: model.skeleton, min: mn, max: mx });
        scene.model_ids.insert(key, scene.models.len() - 1);
        Ok(scene.models.len() - 1)
    }

    /// Static geometry (e.g. a BSP world): one model from ready-made meshes, positions scaled to metres.
    pub fn load_meshes(&self, scene: &mut Scene, vfs: &Vfs, key: &str, meshes: &[mdl::Mesh], scale: f32) -> usize {
        let mut out = Vec::new();
        let (mut mn, mut mx) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for me in meshes {
            let verts: Vec<GpuVertex> = me.vertices.iter().map(|v| GpuVertex { pos: v.pos.map(|c| c * scale), normal: v.normal, uv: v.uv, uv2: v.uv2 }).collect();
            for v in &verts {
                mn = mn.min(Vec3::from(v.pos));
                mx = mx.max(Vec3::from(v.pos));
            }
            let mkey = format!("|{}", me.material);
            let mat = match scene.material_ids.get(&mkey) {
                Some(&i) => i,
                None => {
                    scene.materials.push(self.material(vfs, &[], &me.material));
                    scene.material_ids.insert(mkey, scene.materials.len() - 1);
                    scene.materials.len() - 1
                }
            };
            out.push(DrawMesh {
                vbuf: self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("vb"), contents: bytemuck::cast_slice(&verts), usage: wgpu::BufferUsages::VERTEX }),
                ibuf: self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("ib"), contents: bytemuck::cast_slice(&me.indices), usage: wgpu::BufferUsages::INDEX }),
                n_indices: me.indices.len() as u32,
                material: mat,
            });
        }
        scene.models.push(GpuModel { meshes: out, skin_src: Vec::new(), skeleton: None, min: mn, max: mx });
        scene.model_ids.insert(key.to_string(), scene.models.len() - 1);
        scene.models.len() - 1
    }

    pub fn add_object(&self, scene: &mut Scene, name: &str, model: usize, transform: Mat4, visible: bool) {
        scene.objects.push(Object { name: name.to_string(), model, transform, visible, tint: [1.0; 4], skin: None });
    }

    /// Set one pose parameter of an object (raw value in the parameter's own range) and re-skin it.
    /// Returns false if the object's model has no such parameter.
    pub fn set_pose_param(&self, scene: &mut Scene, obj: usize, name: &str, value: f32) -> bool {
        let model = &scene.models[scene.objects[obj].model];
        let Some(sk) = &model.skeleton else { return false };
        let Some(pi) = sk.param_index(name) else { return false };
        let n_params = sk.pose_params.len();
        let o = &mut scene.objects[obj];
        let model = &scene.models[o.model];
        let skin = o.skin.get_or_insert_with(|| Skin {
            vbufs: model
                .meshes
                .iter()
                .zip(&model.skin_src)
                .map(|(_, src)| {
                    self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("skin"),
                        size: (src.len() * std::mem::size_of::<GpuVertex>()) as u64,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    })
                })
                .collect(),
            params: vec![f32::NAN; n_params],
        });
        if skin.params[pi] == value {
            return true;
        }
        skin.params[pi] = value;
        let params: Vec<f32> = skin.params.iter().map(|p| if p.is_nan() { 0.0 } else { *p }).collect();
        let mats = model.skeleton.as_ref().unwrap().skin_matrices(&params);
        for (buf, src) in skin.vbufs.iter().zip(&model.skin_src) {
            let verts: Vec<GpuVertex> = src
                .iter()
                .map(|v| {
                    let (p, nrm) = (Vec3::from(v.pos), Vec3::from(v.normal));
                    let (mut pp, mut nn) = (Vec3::ZERO, Vec3::ZERO);
                    for k in 0..(v.nbones.max(1) as usize).min(3) {
                        let m = mats.get(v.joints[k] as usize).copied().unwrap_or(Mat4::IDENTITY);
                        let w = if v.nbones <= 1 { 1.0 } else { v.weights[k] };
                        pp += m.transform_point3(p) * w;
                        nn += m.transform_vector3(nrm) * w;
                    }
                    GpuVertex { pos: (pp * SCALE).into(), normal: nn.normalize_or_zero().into(), uv: v.uv, uv2: [0.0; 2] }
                })
                .collect();
            self.queue.write_buffer(buf, 0, bytemuck::cast_slice(&verts));
        }
        true
    }

    pub fn depth_view(&self, w: u32, h: u32) -> wgpu::TextureView {
        self.device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("depth"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: self.samples.get(),
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&Default::default())
    }

    /// Multisampled colour target matching the current sample count (None at 1x).
    pub fn msaa_view(&self, w: u32, h: u32) -> Option<wgpu::TextureView> {
        let n = self.samples.get();
        (n > 1).then(|| {
            self.device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("msaa colour"),
                    size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: n,
                    dimension: wgpu::TextureDimension::D2,
                    format: if self.hdr.get() { crate::post::HDR_FORMAT } else { self.target_format },
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        })
    }

    fn upload_objects(&self, scene: &Scene) {
        let n = scene.objects.len().max(1) as u64;
        if self.object_buf.borrow().1 < n {
            let (buf, bind) = Self::make_object_buffer(&self.device, &self.object_layout, n);
            *self.object_buf.borrow_mut() = (buf, n);
            *self.object_bind.borrow_mut() = bind;
        }
        let mut bytes = vec![0u8; (n * OBJ_STRIDE) as usize];
        for (i, o) in scene.objects.iter().enumerate() {
            let at = i * OBJ_STRIDE as usize;
            bytes[at..at + 64].copy_from_slice(bytemuck::bytes_of(&o.transform.to_cols_array()));
            bytes[at + 64..at + 80].copy_from_slice(bytemuck::bytes_of(&o.tint));
        }
        self.queue.write_buffer(&self.object_buf.borrow().0, 0, &bytes);
    }

    pub fn render(&self, scene: &Scene, view_proj: Mat4, color: &wgpu::TextureView, depth: &wgpu::TextureView) {
        self.render_ms(scene, view_proj, None, color, depth)
    }

    /// Like `render`; with `msaa` the scene is drawn multisampled and resolved into `color`.
    pub fn render_ms(&self, scene: &Scene, view_proj: Mat4, msaa: Option<&wgpu::TextureView>, color: &wgpu::TextureView, depth: &wgpu::TextureView) {
        let pipes = &self.pipes[if self.hdr.get() { 2 } else { (msaa.is_some() && self.samples.get() > 1) as usize }];
        let g = Globals { view_proj: view_proj.to_cols_array_2d(), light: [0.4, -0.3, 0.85, if self.unlit.get() { 1.0 } else { 0.0 }], fx: [self.tonemap.get(), 0.0, 0.0, 0.0] };
        self.queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&g));
        self.upload_objects(scene);
        let obj_bind = self.object_bind.borrow();
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: msaa.unwrap_or(color),
                    depth_slice: None,
                    resolve_target: msaa.map(|_| color),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.05, g: 0.06, b: 0.08, a: 1.0 }),
                        store: if msaa.is_some() { wgpu::StoreOp::Discard } else { wgpu::StoreOp::Store },
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &*self.globals_bind.borrow(), &[]);
            for blend in [Blend::Opaque, Blend::AlphaTest, Blend::Translucent] {
                pass.set_pipeline(match blend {
                    Blend::Opaque => &pipes[0],
                    Blend::AlphaTest => &pipes[1],
                    Blend::Translucent => &pipes[2],
                });
                for (i, o) in scene.objects.iter().enumerate().filter(|(_, o)| o.visible) {
                    pass.set_bind_group(2, &*obj_bind, &[(i as u64 * OBJ_STRIDE) as u32]);
                    for (mi, m) in scene.models[o.model].meshes.iter().enumerate().filter(|(_, m)| scene.materials[m.material].blend == blend) {
                        pass.set_bind_group(1, &scene.materials[m.material].bind, &[]);
                        let vb = o.skin.as_ref().and_then(|s| s.vbufs.get(mi)).unwrap_or(&m.vbuf);
                        pass.set_vertex_buffer(0, vb.slice(..));
                        pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..m.n_indices, 0, 0..1);
                    }
                }
            }
        }
        self.queue.submit(Some(enc.finish()));
    }

    /// Render one frame off-screen and write it as PNG.
    pub fn screenshot(&self, scene: &Scene, view_proj: Mat4, w: u32, h: u32, path: &std::path::Path) -> Result<()> {
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shot"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.target_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        let depth = self.depth_view(w, h);
        self.render(scene, view_proj, &view, &depth);

        let row = (w * 4).next_multiple_of(256);
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit(Some(enc.finish()));
        let slice = buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device.poll(wgpu::PollType::wait_indefinitely())?;
        rx.recv()??;
        let data = slice.get_mapped_range()?;
        let mut pixels = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h as usize {
            pixels.extend_from_slice(&data[y * row as usize..y * row as usize + (w * 4) as usize]);
        }
        let mut enc = png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(path)?), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()?.write_image_data(&pixels)?;
        Ok(())
    }
}

/// A plausible flat colour for a texture we do not have, from keywords in its name (hash for the rest).
fn placeholder_colour(name: &str) -> [u8; 4] {
    let table: [(&str, [u8; 3]); 12] = [
        ("concrete", [150, 148, 140]),
        ("plaster", [190, 180, 160]),
        ("granite", [120, 115, 115]),
        ("marble", [200, 198, 190]),
        ("metal", [110, 118, 128]),
        ("brick", [140, 80, 65]),
        ("tile", [200, 195, 180]),
        ("floor", [130, 125, 118]),
        ("ceil", [170, 168, 160]),
        ("yellow", [200, 170, 70]),
        ("rail", [90, 80, 75]),
        ("wood", [120, 90, 60]),
    ];
    for (k, c) in table {
        if name.contains(k) {
            return [c[0], c[1], c[2], 255];
        }
    }
    let h = name.bytes().fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
    // unknown addon texture: a neutral stone-like grey that varies a little by name
    let v = (h & 0x1f) as u8;
    [150 + v, 146 + v, 138 + v, 255]
}
