mod atlas;
mod geometry_buffer;
mod instance_buffer;

use wgpu::*;

use crate::{Layer, Renderer, Resources, ShaderConstants, ShaderModules};

pub use atlas::*;
pub use geometry_buffer::*;
pub use instance_buffer::*;

pub trait Drawable {
    fn new(renderer: &Renderer) -> Self
    where
        Self: Sized;

    fn name(&self) -> &str;
    fn references<'a>(&'a self) -> Vec<&'a dyn DrawableReference>;

    fn draw<'b, 'a: 'b>(
        &'a mut self,
        queue: &Queue,
        render_pass: &mut RenderPass<'b>,
        constants: ShaderConstants,
        resources: &Resources,
        layer: &Layer,
    );
}

pub trait DrawableReference {
    fn layout(&self) -> Option<BindGroupLayoutEntry> {
        None
    }
    fn entry(&self) -> Option<BindGroupEntry> {
        None
    }
    fn vertex<'b, 'a: 'b>(&'a self) -> Option<VertexBufferLayout<'b>> {
        None
    }
}

pub(crate) struct DrawablePipeline {
    drawable: Box<dyn Drawable>,

    name: String,

    bind_group_layout: BindGroupLayout,
    bind_group: BindGroup,

    render_pipeline: Option<RenderPipeline>,
}

impl DrawablePipeline {
    pub fn new<T: Drawable + 'static>(Renderer { device, .. }: &Renderer, drawable: T) -> Self {
        let drawable = Box::new(drawable);

        let name = drawable.name().to_string();

        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some(&format!("{} bind group layout", &name)),
            entries: drawable
                .references()
                .iter()
                .filter_map(|reference| reference.layout())
                .enumerate()
                .map(|(index, mut layout)| {
                    layout.binding = index as u32;
                    layout
                })
                .collect::<Vec<_>>()
                .as_slice(),
        });

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some(&format!("{} bind group", &name)),
            layout: &bind_group_layout,
            entries: drawable
                .references()
                .iter()
                .filter_map(|reference| reference.entry())
                .enumerate()
                .map(|(index, mut entry)| {
                    entry.binding = index as u32;
                    entry
                })
                .collect::<Vec<_>>()
                .as_slice(),
        });

        Self {
            drawable,
            name,
            bind_group_layout,
            bind_group,
            render_pipeline: None,
        }
    }

    fn try_create_pipeline(
        &self,
        device: &Device,
        shaders: &ShaderModules,
        format: &TextureFormat,
        universal_bind_group_layout: &BindGroupLayout,
    ) -> Result<RenderPipeline, String> {
        let render_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some(&format!("{} Pipeline Layout", self.name)),
            bind_group_layouts: &[&self.bind_group_layout, &universal_bind_group_layout],
            push_constant_ranges: &[PushConstantRange {
                stages: ShaderStages::all(),
                range: 0..std::mem::size_of::<ShaderConstants>() as u32,
            }],
        });

        let vertex_buffer_layouts = self
            .drawable
            .references()
            .iter()
            .filter_map(|reference| reference.vertex())
            .collect::<Vec<_>>();

        Ok(device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some(&format!("{} Pipeline", self.name)),
            layout: Some(&render_pipeline_layout),
            vertex: VertexState {
                module: shaders.get_vertex(&self.name)?,
                entry_point: "main",
                buffers: &vertex_buffer_layouts,
                compilation_options: Default::default(),
            },
            fragment: Some(FragmentState {
                module: shaders.get_fragment(&self.name)?,
                entry_point: "main",
                targets: &[Some(ColorTargetState {
                    format: *format,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: MultisampleState {
                count: 4,
                ..Default::default()
            },
            multiview: None,
        }))
    }

    pub async fn create_pipeline(
        &mut self,
        device: &Device,
        shaders: &ShaderModules,
        format: &TextureFormat,
        universal_bind_group_layout: &BindGroupLayout,
    ) {
        device.push_error_scope(ErrorFilter::Validation);
        let pipeline =
            self.try_create_pipeline(device, shaders, format, universal_bind_group_layout);
        let validation_error = device.pop_error_scope().await;

        if validation_error.is_none() {
            if let Ok(pipeline) = pipeline {
                self.render_pipeline = Some(pipeline);
            }
        }
    }

    pub fn ready(&self) -> bool {
        self.render_pipeline.is_some()
    }

    pub fn draw<'b, 'a: 'b>(
        &'a mut self,
        queue: &Queue,
        render_pass: &mut RenderPass<'b>,
        constants: ShaderConstants,
        universal_bind_group: &'a BindGroup,
        resources: &Resources,
        layer: &Layer,
    ) {
        render_pass.set_pipeline(self.render_pipeline.as_ref().unwrap());

        render_pass.set_push_constants(ShaderStages::all(), 0, bytemuck::cast_slice(&[constants]));

        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_bind_group(1, universal_bind_group, &[]);

        self.drawable
            .draw(queue, render_pass, constants, resources, layer);
    }
}
