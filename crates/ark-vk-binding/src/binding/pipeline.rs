use vulkanalia::prelude::v1_0::*;
use vulkanalia::vk::{self, HasBuilder};
use wasmtime::component::Resource;

use crate::{
    binding::{
        ark::gpu::{
            core::VulkanError,
            pipeline::{
                ComputePipeline, DescriptorSetInfo, GraphicsPipeline,
                GraphicsPipelineCreateInfo, Host, HostComputePipeline, HostGraphicsPipeline,
                HostPipelineLayout, HostRayTracingPipeline, PipelineLayout,
                PrimitiveTopology, RayTracingPipeline,
            },
            shader::{PushConstantRange, ShaderModule},
        },
        vk_err,
    },
    VkContextView,
};

#[repr(transparent)]
pub(crate) struct GpuPipelineLayout {
    pub(crate) layout: vk::PipelineLayout,
}

#[repr(transparent)]
pub(crate) struct GpuComputePipeline {
    pub(crate) pipeline: vk::Pipeline,
}

#[repr(transparent)]
pub(crate) struct GpuGraphicsPipeline {
    pub(crate) pipeline: vk::Pipeline,
}

#[repr(transparent)]
pub(crate) struct GpuRayTracingPipeline {
    pub(crate) pipeline: vk::Pipeline,
}

impl Host for VkContextView<'_> {}

fn vk_push_constant_ranges(ranges: &[PushConstantRange]) -> Vec<vk::PushConstantRange> {
    ranges
        .iter()
        .map(|r| vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::from_bits_truncate(r.stage_flags),
            offset: r.offset,
            size: r.size,
        })
        .collect()
}

// ── HostPipelineLayout ──

impl HostPipelineLayout for VkContextView<'_> {
    fn create(
        &mut self,
        descriptor_sets: Vec<DescriptorSetInfo>,
        push_constant_ranges: Vec<PushConstantRange>,
    ) -> Result<Resource<PipelineLayout>, VulkanError> {
        let mut set_layouts: Vec<vk::DescriptorSetLayout> = Vec::new();
        for ds_info in &descriptor_sets {
            let layout_key =
                Resource::<super::descriptor::GpuDescriptorSetLayout>::new_borrow(ds_info.layout.rep());
            let gpu_dsl = self.table.get(&layout_key).map_err(|_| VulkanError::Unknown)?;
            set_layouts.push(gpu_dsl.layout);
        }

        let push_constant_ranges_vk = vk_push_constant_ranges(&push_constant_ranges);
        let layout_info = vk::PipelineLayoutCreateInfo::builder()
            .set_layouts(&set_layouts)
            .push_constant_ranges(&push_constant_ranges_vk);

        let layout = unsafe { self.vk_device().create_pipeline_layout(&layout_info, None) }
            .map_err(vk_err)?;

        let handle = self.table.push(GpuPipelineLayout { layout })
            .map_err(|_| VulkanError::OutOfHostMemory)?;
        Ok(Resource::new_own(handle.rep()))
    }

    fn drop(&mut self, rep: Resource<PipelineLayout>) -> wasmtime::anyhow::Result<()> {
        let key = Resource::<GpuPipelineLayout>::new_own(rep.rep());
        let layout = self.table.delete(key)?;
        unsafe { self.vk_device().destroy_pipeline_layout(layout.layout, None); }
        Ok(())
    }
}

// ── HostComputePipeline ──

impl HostComputePipeline for VkContextView<'_> {
    fn create(
        &mut self,
        layout: Resource<PipelineLayout>,
        shader: Resource<ShaderModule>,
        entry_point: String,
    ) -> Result<Resource<ComputePipeline>, VulkanError> {
        let layout_key = Resource::<GpuPipelineLayout>::new_borrow(layout.rep());
        let gpu_layout = self.table.get(&layout_key).map_err(|_| VulkanError::Unknown)?;

        let shader_key = Resource::<super::shader::GpuShaderModule>::new_borrow(shader.rep());
        let gpu_shader = self.table.get(&shader_key).map_err(|_| VulkanError::Unknown)?;

        let entry_name = std::ffi::CString::new(entry_point)
            .map_err(|_| VulkanError::Unnamed("entry point name contains null byte".into()))?;

        let stage = vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(gpu_shader.module)
            .name(entry_name.as_bytes_with_nul());

        let pipeline_info = vk::ComputePipelineCreateInfo::builder()
            .stage(stage)
            .layout(gpu_layout.layout);

        let (pipelines, _) = unsafe {
            self.vk_device().create_compute_pipelines(
                vk::PipelineCache::default(), &[pipeline_info.build()], None,
            )
        }
        .map_err(vk_err)?;

        let pipeline = pipelines.into_iter().next()
            .ok_or_else(|| VulkanError::Unnamed("driver returned zero pipelines".into()))?;

        let handle = self.table.push(GpuComputePipeline { pipeline })
            .map_err(|_| VulkanError::OutOfHostMemory)?;
        Ok(Resource::new_own(handle.rep()))
    }

    fn drop(&mut self, rep: Resource<ComputePipeline>) -> wasmtime::anyhow::Result<()> {
        let key = Resource::<GpuComputePipeline>::new_own(rep.rep());
        let pipeline = self.table.delete(key)?;
        unsafe { self.vk_device().destroy_pipeline(pipeline.pipeline, None); }
        Ok(())
    }
}

// ── HostGraphicsPipeline ──

impl HostGraphicsPipeline for VkContextView<'_> {
    fn create(
        &mut self,
        info: GraphicsPipelineCreateInfo,
    ) -> Result<Resource<GraphicsPipeline>, VulkanError> {
        let layout_key = Resource::<GpuPipelineLayout>::new_borrow(info.layout.rep());
        let gpu_layout = self.table.get(&layout_key).map_err(|_| VulkanError::Unknown)?;

        let vs_key = Resource::<super::shader::GpuShaderModule>::new_borrow(info.vertex_shader.rep());
        let vs = self.table.get(&vs_key).map_err(|_| VulkanError::Unknown)?;
        let fs_key = Resource::<super::shader::GpuShaderModule>::new_borrow(info.fragment_shader.rep());
        let fs = self.table.get(&fs_key).map_err(|_| VulkanError::Unknown)?;

        let vs_entry = std::ffi::CString::new(info.vertex_entry)
            .map_err(|_| VulkanError::Unnamed("vertex entry name contains null byte".into()))?;
        let fs_entry = std::ffi::CString::new(info.fragment_entry)
            .map_err(|_| VulkanError::Unnamed("fragment entry name contains null byte".into()))?;

        let stages = [
            vk::PipelineShaderStageCreateInfo {
                s_type: vk::StructureType::PIPELINE_SHADER_STAGE_CREATE_INFO,
                next: std::ptr::null(),
                flags: vk::PipelineShaderStageCreateFlags::empty(),
                stage: vk::ShaderStageFlags::VERTEX,
                module: vs.module,
                name: vs_entry.as_ptr(),
                specialization_info: std::ptr::null(),
            },
            vk::PipelineShaderStageCreateInfo {
                s_type: vk::StructureType::PIPELINE_SHADER_STAGE_CREATE_INFO,
                next: std::ptr::null(),
                flags: vk::PipelineShaderStageCreateFlags::empty(),
                stage: vk::ShaderStageFlags::FRAGMENT,
                module: fs.module,
                name: fs_entry.as_ptr(),
                specialization_info: std::ptr::null(),
            },
        ];

        let vk_attrs: Vec<vk::VertexInputAttributeDescription> = info.vertex_attributes.iter()
            .map(|a| vk::VertexInputAttributeDescription {
                location: a.location, binding: a.binding,
                format: vk::Format::from_raw(a.format as i32), offset: a.offset,
            }).collect();
        let vk_bindings: Vec<vk::VertexInputBindingDescription> = {
            let mut bs: Vec<u32> = info.vertex_attributes.iter().map(|a| a.binding).collect();
            bs.sort(); bs.dedup();
            bs.into_iter().map(|b| vk::VertexInputBindingDescription {
                binding: b, stride: 0, input_rate: vk::VertexInputRate::VERTEX,
            }).collect()
        };

        let vertex_input = vk::PipelineVertexInputStateCreateInfo::builder()
            .vertex_binding_descriptions(&vk_bindings)
            .vertex_attribute_descriptions(&vk_attrs);

        let topology = match info.topology {
            PrimitiveTopology::PointList => vk::PrimitiveTopology::POINT_LIST,
            PrimitiveTopology::LineList => vk::PrimitiveTopology::LINE_LIST,
            PrimitiveTopology::LineStrip => vk::PrimitiveTopology::LINE_STRIP,
            PrimitiveTopology::TriangleList => vk::PrimitiveTopology::TRIANGLE_LIST,
            PrimitiveTopology::TriangleStrip => vk::PrimitiveTopology::TRIANGLE_STRIP,
            PrimitiveTopology::TriangleFan => vk::PrimitiveTopology::TRIANGLE_FAN,
        };

        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder().topology(topology);
        let viewport_state = vk::PipelineViewportStateCreateInfo::builder().viewport_count(1).scissor_count(1);
        let rasterization = vk::PipelineRasterizationStateCreateInfo::builder()
            .polygon_mode(vk::PolygonMode::FILL).line_width(1.0)
            .cull_mode(vk::CullModeFlags::NONE).front_face(vk::FrontFace::CLOCKWISE);
        let multisample = vk::PipelineMultisampleStateCreateInfo::builder()
            .rasterization_samples(vk::SampleCountFlags::_1);

        let color_blend_attachment = vk::PipelineColorBlendAttachmentState::builder()
            .color_write_mask(vk::ColorComponentFlags::R | vk::ColorComponentFlags::G | vk::ColorComponentFlags::B | vk::ColorComponentFlags::A);
        let color_blend = vk::PipelineColorBlendStateCreateInfo::builder()
            .attachments(std::slice::from_ref(&color_blend_attachment));
        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic_state = vk::PipelineDynamicStateCreateInfo::builder().dynamic_states(&dynamic_states);

        let mut rendering_info = vk::PipelineRenderingCreateInfo::builder()
            .color_attachment_formats(&[vk::Format::from_raw(info.color_format as i32)])
            .build();

        let pipeline_info = vk::GraphicsPipelineCreateInfo::builder()
            .stages(&stages).vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly).viewport_state(&viewport_state)
            .rasterization_state(&rasterization).multisample_state(&multisample)
            .color_blend_state(&color_blend).dynamic_state(&dynamic_state)
            .layout(gpu_layout.layout)
            .push_next(&mut rendering_info);

        let (pipelines, _) = unsafe {
            self.vk_device().create_graphics_pipelines(
                vk::PipelineCache::default(), &[pipeline_info.build()], None,
            )
        }
        .map_err(vk_err)?;

        let pipeline = pipelines.into_iter().next()
            .ok_or_else(|| VulkanError::Unnamed("driver returned zero pipelines".into()))?;

        let handle = self.table.push(GpuGraphicsPipeline { pipeline })
            .map_err(|_| VulkanError::OutOfHostMemory)?;
        Ok(Resource::new_own(handle.rep()))
    }

    fn drop(&mut self, rep: Resource<GraphicsPipeline>) -> wasmtime::anyhow::Result<()> {
        let key = Resource::<GpuGraphicsPipeline>::new_own(rep.rep());
        let pipeline = self.table.delete(key)?;
        unsafe { self.vk_device().destroy_pipeline(pipeline.pipeline, None); }
        Ok(())
    }
}

// ── HostRayTracingPipeline ──

impl HostRayTracingPipeline for VkContextView<'_> {
    fn drop(&mut self, rep: Resource<RayTracingPipeline>) -> wasmtime::anyhow::Result<()> {
        let key = Resource::<GpuRayTracingPipeline>::new_own(rep.rep());
        let pipeline = self.table.delete(key)?;
        unsafe { self.vk_device().destroy_pipeline(pipeline.pipeline, None); }
        Ok(())
    }
}
