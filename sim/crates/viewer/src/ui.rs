//! 2D overlay: coloured rectangles and text from a pre-rendered font atlas (`tools/make_font.py`).

use serde_json::Value;
use std::collections::HashMap;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct V {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

pub type Color = [f32; 4];

pub struct Ui {
    pipeline: wgpu::RenderPipeline,
    screen_buf: wgpu::Buffer,
    screen_bind: wgpu::BindGroup,
    atlas_bind: wgpu::BindGroup,
    verts: Vec<V>,
    glyphs: HashMap<u32, (u32, f32)>,
    cell: (f32, f32),
    cols: u32,
    pad: f32,
    size: f32,
    atlas: (f32, f32),
    white: (f32, f32),
    pub screen: (f32, f32),
}

impl Ui {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, target_format: wgpu::TextureFormat) -> Ui {
        let meta: Value = serde_json::from_str(include_str!("../ui_font.json")).unwrap();
        let glyphs = meta["glyphs"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.parse().unwrap(), (v[0].as_u64().unwrap() as u32, v[1].as_f64().unwrap() as f32)))
            .collect();
        let f = |v: &Value, i: usize| v[i].as_f64().unwrap() as f32;
        let img = {
            let dec = png::Decoder::new(std::io::Cursor::new(&include_bytes!("../ui_font.png")[..]));
            let mut r = dec.read_info().unwrap();
            let mut buf = vec![0; r.output_buffer_size().unwrap()];
            let info = r.next_frame(&mut buf).unwrap();
            buf.truncate(info.buffer_size());
            (buf, info.width, info.height)
        };
        let tex = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("ui font"),
                size: wgpu::Extent3d { width: img.1, height: img.2, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &img.0,
        );
        let view = tex.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui"),
            source: wgpu::ShaderSource::Wgsl(include_str!("ui.wgsl").into()),
        });
        let screen_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui screen"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let atlas_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui atlas"),
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
        let screen_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui screen"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let screen_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &screen_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: screen_buf.as_entire_binding() }],
        });
        let atlas_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &atlas_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&screen_layout), Some(&atlas_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<V>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4],
                })],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Ui {
            pipeline,
            screen_buf,
            screen_bind,
            atlas_bind,
            verts: Vec::new(),
            glyphs,
            cell: (f(&meta["cell"], 0), f(&meta["cell"], 1)),
            cols: meta["cols"].as_u64().unwrap() as u32,
            pad: meta["pad"].as_f64().unwrap() as f32,
            size: meta["size"].as_f64().unwrap() as f32,
            atlas: (f(&meta["atlas"], 0), f(&meta["atlas"], 1)),
            white: (f(&meta["white"], 0), f(&meta["white"], 1)),
            screen: (1600.0, 900.0),
        }
    }

    fn quad(&mut self, x: f32, y: f32, w: f32, h: f32, uv: (f32, f32, f32, f32), c: Color) {
        let p = |px: f32, py: f32, u: f32, v: f32| V { pos: [px, py], uv: [u / self.atlas.0, v / self.atlas.1], color: c };
        let (u0, v0, u1, v1) = uv;
        let q = [p(x, y, u0, v0), p(x + w, y, u1, v0), p(x + w, y + h, u1, v1), p(x, y + h, u0, v1)];
        self.verts.extend_from_slice(&[q[0], q[1], q[2], q[0], q[2], q[3]]);
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: Color) {
        let (u, v) = self.white;
        self.quad(x, y, w, h, (u, v, u, v), c);
    }

    /// Width in pixels of `s` at pixel height `px` (the font's em size).
    pub fn text_width(&self, s: &str, px: f32) -> f32 {
        let k = px / self.size;
        s.chars().map(|ch| self.glyphs.get(&(ch as u32)).or_else(|| self.glyphs.get(&63)).map_or(0.0, |g| g.1) * k).sum()
    }

    pub fn text(&mut self, x: f32, y: f32, px: f32, s: &str, c: Color) {
        let k = px / self.size;
        let mut cx = x;
        for ch in s.chars() {
            let Some(&(idx, adv)) = self.glyphs.get(&(ch as u32)).or_else(|| self.glyphs.get(&63)) else { continue };
            if ch != ' ' {
                let (gx, gy) = ((idx % self.cols) as f32 * self.cell.0, (idx / self.cols) as f32 * self.cell.1);
                self.quad(cx - self.pad * k, y - self.pad * k, self.cell.0 * k, self.cell.1 * k, (gx, gy, gx + self.cell.0, gy + self.cell.1), c);
            }
            cx += adv * k;
        }
    }

    /// Draw everything queued since the last call on top of `target`.
    pub fn flush(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, target: &wgpu::TextureView) {
        if self.verts.is_empty() {
            return;
        }
        queue.write_buffer(&self.screen_buf, 0, bytemuck::cast_slice(&[self.screen.0, self.screen.1, 0.0, 0.0]));
        let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ui verts"),
            contents: bytemuck::cast_slice(&self.verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.screen_bind, &[]);
            pass.set_bind_group(1, &self.atlas_bind, &[]);
            pass.set_vertex_buffer(0, vb.slice(..));
            pass.draw(0..self.verts.len() as u32, 0..1);
        }
        queue.submit(Some(enc.finish()));
        self.verts.clear();
    }
}
