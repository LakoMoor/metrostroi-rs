//! HDR scene target and the bloom / tone-mapping chain of the ultra quality tier.

pub const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

pub struct Post {
    /// The resolved HDR scene colour (the main pass resolves into it).
    pub scene: wgpu::TextureView,
    half: [wgpu::TextureView; 2],
    sampler: wgpu::Sampler,
    layout: wgpu::BindGroupLayout,
    bright: wgpu::RenderPipeline,
    blur_h: wgpu::RenderPipeline,
    blur_v: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    /// Stand-in second texture for the passes that do not use `bloom`.
    dummy: wgpu::TextureView,
}

fn texture(device: &wgpu::Device, w: u32, h: u32, usage: wgpu::TextureUsages) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("post"),
            size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

impl Post {
    /// `w`, `h` is the render resolution; the composite pass stretches it over whatever it is run into.
    pub fn new(device: &wgpu::Device, out_format: wgpu::TextureFormat, w: u32, h: u32) -> Post {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("post"), source: wgpu::ShaderSource::Wgsl(include_str!("post.wgsl").into()) });
        let tex = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post layout"),
            entries: &[tex(0), wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None }, tex(2)],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("post pl"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let make = |fs: &str, format: wgpu::TextureFormat| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(fs),
                layout: Some(&pl),
                vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let rt = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        Post {
            scene: texture(device, w, h, rt),
            half: [texture(device, w / 4, h / 4, rt), texture(device, w / 4, h / 4, rt)],
            sampler,
            layout,
            bright: make("fs_bright", HDR_FORMAT),
            blur_h: make("fs_blur_h", HDR_FORMAT),
            blur_v: make("fs_blur_v", HDR_FORMAT),
            composite: make("fs_composite", out_format),
            dummy: texture(device, 1, 1, rt),
        }
    }

    fn pass(&self, device: &wgpu::Device, enc: &mut wgpu::CommandEncoder, pipe: &wgpu::RenderPipeline, a: &wgpu::TextureView, b: &wgpu::TextureView, out: &wgpu::TextureView) {
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post bind"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(a) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(b) },
            ],
        });
        let mut p = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("post"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: out, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        p.set_pipeline(pipe);
        p.set_bind_group(0, &bind, &[]);
        p.draw(0..3, 0..1);
    }

    /// Bloom and composite the resolved HDR scene into `out`.
    pub fn run(&self, device: &wgpu::Device, queue: &wgpu::Queue, out: &wgpu::TextureView) {
        let mut enc = device.create_command_encoder(&Default::default());
        self.pass(device, &mut enc, &self.bright, &self.scene, &self.dummy, &self.half[0]);
        self.pass(device, &mut enc, &self.blur_h, &self.half[0], &self.dummy, &self.half[1]);
        self.pass(device, &mut enc, &self.blur_v, &self.half[1], &self.dummy, &self.half[0]);
        self.pass(device, &mut enc, &self.composite, &self.scene, &self.half[0], out);
        queue.submit(Some(enc.finish()));
    }
}
