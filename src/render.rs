use std::{
    ffi::CString,
    str::FromStr,
    sync::{Arc, Mutex},
};
use wgpu::RenderPassDepthStencilAttachment;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowAttributes},
};

pub struct Live2DDrawCall {
    pub vertices: Vec<crate::model_interaction::Live2DVertex>,
    pub indices: Vec<u16>,
    pub texture_idx: i32,
    pub blend_mode: i32,
    pub render_order: i32,
    pub opacity: f32,
    pub masks: Vec<i32>,
    pub source_index: i32,
}

pub struct State {
    //util
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    window: Arc<Window>,

    //rendering
    pipelines: Live2DPipelines,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    texture_bind_groups: Vec<wgpu::BindGroup>,
    uniform_bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    all_indices: Vec<u16>,
    all_vertices: Vec<crate::model_interaction::Live2DVertex>,
    draw_call_offsets: Vec<(u32, u32, u32)>,

    //anims
    last_parameters_hash: Vec<(String, f32)>,

    //masks
    depth_stencil_view: wgpu::TextureView,
}

use serde::Deserialize;
use std::fs::File;
use std::path::Path;

use crate::{
    animations::{self, SharedAnimationState},
    model_interaction,
};

#[derive(Deserialize, Debug)]
struct Model3Json {
    #[serde(rename = "FileReferences")]
    file_references: FileReferences,
}

#[derive(Deserialize, Debug)]
struct FileReferences {
    #[serde(rename = "Moc")]
    moc: String,
    #[serde(rename = "Textures")]
    textures: Vec<String>,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Live2DUniforms {
    pub projection: [f32; 16],
    pub base_color: [f32; 4],
}

fn get_texture_paths(model_json_path: &str) -> Vec<String> {
    let file = File::open(model_json_path).expect("Failed to open .model3.json");
    let json: Model3Json = serde_json::from_reader(file).expect("Failed to parse JSON");

    let base_dir = Path::new(model_json_path).parent().unwrap();

    json.file_references
        .textures
        .iter()
        .map(|tex_rel_path| base_dir.join(tex_rel_path).to_string_lossy().into_owned())
        .collect()
}

fn load_texture_to_bind_group(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    path: &str,
) -> wgpu::BindGroup {
    let img = image::open(path).expect(&format!("Failed to open texture: {}", path));
    let rgba = img.to_rgba8();
    let dimensions = rgba.dimensions();

    println!("Loading texture {} into VRAM.", path);

    let texture_size = wgpu::Extent3d {
        width: dimensions.0,
        height: dimensions.1,
        depth_or_array_layers: 1,
    };

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(path),
        size: texture_size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &rgba,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(4 * dimensions.0),
            rows_per_image: Some(dimensions.1),
        },
        texture_size,
    );

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(&format!("Bind Group for {}", path)),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

impl State {
    fn compute_projection_matrix(&self) -> [f32; 16] {
        let window_aspect = self.size.width as f32 / self.size.height as f32;

        let scale_x = 5.0;
        let scale_y = 5.0;

        let (final_scale_x, final_scale_y) = if window_aspect > 1.0 {
            (1.0 / window_aspect, scale_y)
        } else {
            (scale_x, scale_y * window_aspect)
        };

        [
            final_scale_x,
            0.0,
            0.0,
            0.0,
            0.0,
            final_scale_y,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        ]
    }

    async fn new(window: Window) -> Self {
        let manifest_dir =
            std::env::var("CARGO_MANIFEST_DIR").expect("Failed to get CARGO_MANIFEST_DIR");

        let window = Arc::new(window);
        let size = window.inner_size();

        let instance = wgpu::Instance::default();

        let surface = instance.create_surface(Arc::clone(&window)).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Device Descriptor"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Live2D Texture Bind Group Layout"),
                entries: &[
                    // Текстура
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // Сэмплер
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Live2D Texture Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let model_json = format!("{}/models/runtime/zundamon.model3.json", manifest_dir);
        let paths = get_texture_paths(&model_json);

        println!("Found {} textures.", paths.len());

        let texture_bind_groups = paths
            .iter()
            .map(|path| {
                load_texture_to_bind_group(
                    &device,
                    &queue,
                    &texture_bind_group_layout,
                    &sampler,
                    path,
                )
            })
            .collect::<Vec<_>>();

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Live2D Uniform Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Live2D Uniform Buffer"),
            size: std::mem::size_of::<Live2DUniforms>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let initial_projection = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];

        let uniforms = Live2DUniforms {
            projection: initial_projection,
            base_color: [1.0, 1.0, 1.0, 1.0],
        };

        queue.write_buffer(&uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Live2D Uniform Bind Group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Live2D Pipeline Layout"),
            bind_group_layouts: &[&uniform_bind_group_layout, &texture_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipelines = Live2DPipelines::new(&device, &pipeline_layout, surface_format);

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Live2D Dynamic Vertex Buffer"),
            size: 10 * 1024 * 1024,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Live2D Dynamic Index Buffer"),
            size: 2 * 1024 * 1024,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let depth_stencil_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Live2D Depth Stencil Texture"),
            size: wgpu::Extent3d {
                width: config.width,
                height: config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24PlusStencil8,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_stencil_view =
            depth_stencil_texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            surface,
            device,
            queue,
            config,
            size,
            window,
            pipelines,
            vertex_buffer,
            index_buffer,
            texture_bind_groups: texture_bind_groups,
            uniform_bind_group: uniform_bind_group,
            uniform_buffer: uniform_buffer,

            last_parameters_hash: vec![],

            depth_stencil_view: depth_stencil_view,
            all_indices: Vec::new(),
            all_vertices: Vec::new(),
            draw_call_offsets: Vec::new(),
        }
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);

            let updated_projection = self.compute_projection_matrix();
            let uniforms = Live2DUniforms {
                projection: updated_projection,
                base_color: [1.0, 1.0, 1.0, 1.0],
            };

            let depth_stencil_texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Live2D Depth Stencil Texture (Resize)"),
                size: wgpu::Extent3d {
                    width: new_size.width,
                    height: new_size.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth24PlusStencil8,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });

            self.depth_stencil_view =
                depth_stencil_texture.create_view(&wgpu::TextureViewDescriptor::default());

            self.queue
                .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
        }
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let frame_snapshot = {
            let shared_data = animations::anim_state.current_frame.lock().unwrap();
            shared_data.clone()
        };

        unsafe {
            for (param_id, param_value) in &frame_snapshot.parameters {
                let current_value = crate::model_interaction::getParameterValue(*param_id);
                let blend_speed = 0.15f32;
                let smoothed_value = current_value + (param_value - current_value) * blend_speed;
                model_interaction::setParameterValue(*param_id, smoothed_value);
            }
            model_interaction::updateModel();
        }

        let draw_calls = unsafe { crate::model_interaction::process_live2d_frame() };

        if draw_calls.is_empty() {
            return Ok(());
        }

        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        let index_size = std::mem::size_of::<u16>();
        let mut all_vertices = &mut self.all_vertices;
        let mut all_indices = &mut self.all_indices;
        let mut draw_call_offsets = &mut self.draw_call_offsets;

        let mut vertex_offset = 0u32;
        let mut index_offset = 0u32;

        for dc in &draw_calls {
            draw_call_offsets.push((vertex_offset, index_offset, dc.indices.len() as u32));

            for vertex in &dc.vertices {
                all_vertices.push(crate::model_interaction::Live2DVertex {
                    position: vertex.position,
                    uv: vertex.uv,
                    opacity: dc.opacity,
                });
            }

            all_indices.extend_from_slice(&dc.indices);

            vertex_offset += dc.vertices.len() as u32;
            index_offset += dc.indices.len() as u32;
        }

        self.queue
            .write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&all_vertices));
        self.queue
            .write_buffer(&self.index_buffer, 0, bytemuck::cast_slice(&all_indices));

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Live2D Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.10,
                            b: 0.15,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                timestamp_writes: None,
                occlusion_query_set: None,
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_stencil_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0),
                        store: wgpu::StoreOp::Store,
                    }),
                }),
            });

            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);

            for (dc_idx, &(v_offset, i_offset, i_count)) in draw_call_offsets.iter().enumerate() {
                let dc = &draw_calls[dc_idx];

                if dc.opacity <= 0.001 {
                    continue;
                }

                if !dc.masks.is_empty() {
                    let mut any_mask_drawn: bool = false;

                    render_pass.set_pipeline(&self.pipelines.mask_write);
                    render_pass.set_stencil_reference(0);
                    for &mask_id in &dc.masks {
                        if let Some(mask_idx) =
                            draw_calls.iter().position(|x| x.source_index == mask_id)
                        {
                            let &(m_v_offset, m_i_offset, m_i_count) = &draw_call_offsets[mask_idx];
                            let m_i_start = (m_i_offset as u64) * (index_size as u64);
                            let m_i_end = m_i_start + ((m_i_count as u64) * (index_size as u64));
                            render_pass.set_index_buffer(
                                self.index_buffer.slice(m_i_start..m_i_end),
                                wgpu::IndexFormat::Uint16,
                            );
                            render_pass.draw_indexed(0..m_i_count, m_v_offset as i32, 0..1);
                            any_mask_drawn = false;
                        }
                    }

                    if any_mask_drawn {
                        render_pass.set_pipeline(&self.pipelines.mask_read);
                        render_pass.set_stencil_reference(1);
                    } else {
                        match dc.blend_mode {
                            1 => render_pass.set_pipeline(&self.pipelines.additive),
                            2 => render_pass.set_pipeline(&self.pipelines.multiply),
                            _ => render_pass.set_pipeline(&self.pipelines.normal),
                        }
                    }
                } else {
                    match dc.blend_mode {
                        1 => render_pass.set_pipeline(&self.pipelines.additive),
                        2 => render_pass.set_pipeline(&self.pipelines.multiply),
                        _ => render_pass.set_pipeline(&self.pipelines.normal),
                    }
                }

                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);

                if (dc.texture_idx as usize) < self.texture_bind_groups.len() {
                    render_pass.set_bind_group(
                        1,
                        &self.texture_bind_groups[dc.texture_idx as usize],
                        &[],
                    );
                }

                let index_byte_start = (i_offset as u64) * (index_size as u64);
                let index_byte_end = index_byte_start + ((i_count as u64) * (index_size as u64));

                render_pass.set_index_buffer(
                    self.index_buffer.slice(index_byte_start..index_byte_end),
                    wgpu::IndexFormat::Uint16,
                );

                render_pass.draw_indexed(0..i_count, v_offset as i32, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        all_vertices.clear();
        all_indices.clear();
        draw_call_offsets.clear();

        Ok(())
    }
}

pub struct Live2DPipelines {
    pub normal: wgpu::RenderPipeline,
    pub additive: wgpu::RenderPipeline,
    pub multiply: wgpu::RenderPipeline,
    pub mask_write: wgpu::RenderPipeline,
    pub mask_read: wgpu::RenderPipeline,
}

impl Live2DPipelines {
    pub fn new(
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        texture_format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Live2D Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/live2d.wgsl").into()),
        });

        let vertex_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<crate::model_interaction::Live2DVertex>()
                as wgpu::BufferAddress, // 20
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2, // position
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2, // uv
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32, // opacity
                },
            ],
        };

        let default_depth_stencil = wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth24PlusStencil8,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::Always,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        };

        let create_pipeline =
            |label: &str, blend_state: wgpu::BlendState, depth_stencil: wgpu::DepthStencilState| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: "vs_main",
                        buffers: &[vertex_buffer_layout.clone()],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: "fs_main",
                        targets: &[Some(wgpu::ColorTargetState {
                            format: texture_format,
                            blend: Some(blend_state),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: Default::default(),
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        ..Default::default()
                    },
                    depth_stencil: Some(depth_stencil),
                    multisample: wgpu::MultisampleState::default(),
                    multiview: None,
                    cache: None,
                })
            };

        let normal_blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };

        let additive_blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };

        let multiply_blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Dst,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };

        let mask_write_stencil = wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth24PlusStencil8,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::Always,
            stencil: wgpu::StencilState {
                front: wgpu::StencilFaceState {
                    compare: wgpu::CompareFunction::Always,
                    fail_op: wgpu::StencilOperation::Keep,
                    depth_fail_op: wgpu::StencilOperation::Keep,
                    pass_op: wgpu::StencilOperation::Replace,
                },
                back: wgpu::StencilFaceState::default(),
                read_mask: 0xFF,
                write_mask: 0xFF,
            },
            bias: wgpu::DepthBiasState::default(),
        };

        let mask_write = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Live2D Mask Write Pipeline"),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[vertex_buffer_layout.clone()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: texture_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::empty(),
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: Some(mask_write_stencil),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let mask_read_stencil = wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth24PlusStencil8,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::Always,
            stencil: wgpu::StencilState {
                front: wgpu::StencilFaceState {
                    compare: wgpu::CompareFunction::Equal,
                    fail_op: wgpu::StencilOperation::Keep,
                    depth_fail_op: wgpu::StencilOperation::Keep,
                    pass_op: wgpu::StencilOperation::Zero,
                },
                back: wgpu::StencilFaceState::default(),
                read_mask: 0xFF,
                write_mask: 0xFF,
            },
            bias: wgpu::DepthBiasState::default(),
        };

        let mask_read = create_pipeline("Live2D Mask Read", normal_blend, mask_read_stencil);

        Self {
            normal: create_pipeline(
                "Live2D Normal Pipeline",
                normal_blend,
                default_depth_stencil.clone(),
            ),
            additive: create_pipeline(
                "Live2D Additive Pipeline",
                additive_blend,
                default_depth_stencil.clone(),
            ),
            multiply: create_pipeline(
                "Live2D Multiply Pipeline",
                multiply_blend,
                default_depth_stencil.clone(),
            ),
            mask_write,
            mask_read,
        }
    }
}

#[derive(Default)]
pub struct App {
    pub state: Option<State>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.state.is_none() {
            let window_attributes = WindowAttributes::default().with_title("Live2D wgpu Window");
            let window = event_loop.create_window(window_attributes).unwrap();

            let state = pollster::block_on(State::new(window));
            self.state = Some(state);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let state = match self.state.as_mut() {
            Some(state) => state,
            None => return,
        };

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(physical_size) => {
                state.resize(physical_size);
            }
            WindowEvent::RedrawRequested => {
                match state.render() {
                    Ok(_) => {}
                    Err(wgpu::SurfaceError::Lost) => state.resize(state.size),
                    Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                    Err(e) => eprintln!("{:?}", e),
                }
                state.window.request_redraw();
            }
            WindowEvent::CursorMoved {
                device_id,
                position,
            } => {
                // self.shared_anim_state
                //     .current_frame
                //     .lock()
                //     .unwrap()
                //     .parameters = vec![
                //     (71, (position.x / 100.0) as f32),
                //     (87, (position.x / 100.0) as f32),
                //     (0, (position.x / 22.0) as f32),
                //     (1, (position.y / -25.0) as f32 + 30.0),
                //     (2, (position.x / 50.0) as f32),
                //     (20, (position.x / 900.0) as f32),
                //     (21, (position.y / 1200.0) as f32 - 0.5),
                // ];
            }
            _ => {}
        }
    }
}
