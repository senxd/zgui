use rustc_hash::FxHashMap;
use wgpu::util::DeviceExt;

pub(crate) fn render(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    pipelines: &mut FxHashMap<&'static str, wgpu::ComputePipeline>,
    width: u32,
    height: u32,
    image: &zgui::image::ProceduralImage,
) -> wgpu::Texture {
    let pipeline = pipelines.entry(image.shader).or_insert_with(|| {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("procedural image"),
            source: wgpu::ShaderSource::Wgsl(image.shader.into()),
        });
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("procedural image"),
            layout: None,
            module: &module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        })
    });
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("procedural image"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let parameters = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("procedural uniforms"),
        contents: bytemuck::cast_slice(&image.parameters),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("procedural image"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: parameters.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(
                    &texture.create_view(&Default::default()),
                ),
            },
        ],
    });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups(image.dispatch[0], image.dispatch[1], 1);
    }
    texture
}
