use vulkanalia::prelude::v1_0::*;
use vulkanalia::vk::{self, HasBuilder};
use wasmtime::component::Resource;

use crate::{
    binding::{
        ark::gpu::{
            core::VulkanError,
            pipeline::{
                ComputePipeline, DescriptorSetInfo, GraphicsPipeline, Host, HostComputePipeline,
                HostGraphicsPipeline, HostPipelineLayout, HostRayTracingPipeline, PipelineLayout,
                RayTracingPipeline,
            },
            shader::{PushConstantRange, ShaderModule},
        },
        vk_err,
    },
    VkContextView,
};

pub(crate) struct GpuPipelineLayout {
    pub(crate) layout: vk::PipelineLayout,
}

pub(crate) struct GpuComputePipeline {
    pub(crate) pipeline: vk::Pipeline,
}

pub(crate) struct GpuGraphicsPipeline {
    pub(crate) pipeline: vk::Pipeline,
}

pub(crate) struct GpuRayTracingPipeline {
    pub(crate) pipeline: vk::Pipeline,
}

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

impl Host for VkContextView<'_> {
    fn create_pipeline_layout(
        &mut self,
        descriptor_sets: Vec<DescriptorSetInfo>,
        push_constant_ranges: Vec<PushConstantRange>,
    ) -> Result<Resource<PipelineLayout>, VulkanError> {
        // Collect descriptor set layouts.
        let mut set_layouts: Vec<vk::DescriptorSetLayout> = Vec::new();

        for ds_info in &descriptor_sets {
            let layout_key =
                Resource::<super::descriptor::GpuDescriptorSetLayout>::new_borrow(ds_info.layout.rep());
            let gpu_dsl = self
                .table
                .get(&layout_key)
                .map_err(|_| VulkanError::Unknown)?;
            set_layouts.push(gpu_dsl.layout);
        }

        let push_constant_ranges_vk = vk_push_constant_ranges(&push_constant_ranges);

        let layout_info = vk::PipelineLayoutCreateInfo::builder()
            .set_layouts(&set_layouts)
            .push_constant_ranges(&push_constant_ranges_vk);

        let layout = unsafe { self.vk_device().create_pipeline_layout(&layout_info, None) }
            .map_err(vk_err)?;

        let handle = self
            .table
            .push(GpuPipelineLayout { layout })
            .map_err(|_| VulkanError::OutOfHostMemory)?;
        Ok(Resource::new_own(handle.rep()))
    }

    fn create_compute_pipeline(
        &mut self,
        layout: Resource<PipelineLayout>,
        shader: Resource<ShaderModule>,
        entry_point: String,
    ) -> Result<Resource<ComputePipeline>, VulkanError> {
        let layout_key = Resource::<GpuPipelineLayout>::new_borrow(layout.rep());
        let gpu_layout = self
            .table
            .get(&layout_key)
            .map_err(|_| VulkanError::Unknown)?;

        let shader_key = Resource::<super::shader::GpuShaderModule>::new_borrow(shader.rep());
        let gpu_shader = self
            .table
            .get(&shader_key)
            .map_err(|_| VulkanError::Unknown)?;

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
                vk::PipelineCache::default(),
                &[pipeline_info.build()],
                None,
            )
        }
        .map_err(vk_err)?;

        // Safe access: even though VK_SUCCESS guarantees at least one
        // pipeline, a buggy driver could return an empty vec.
        let pipeline = pipelines
            .into_iter()
            .next()
            .ok_or_else(|| VulkanError::Unnamed("driver returned zero pipelines".into()))?;

        let handle = self
            .table
            .push(GpuComputePipeline { pipeline })
            .map_err(|_| VulkanError::OutOfHostMemory)?;
        Ok(Resource::new_own(handle.rep()))
    }
}

impl HostPipelineLayout for VkContextView<'_> {
    fn drop(&mut self, rep: Resource<PipelineLayout>) -> wasmtime::anyhow::Result<()> {
        let key = Resource::<GpuPipelineLayout>::new_own(rep.rep());
        let layout = self.table.delete(key)?;
        unsafe {
            self.vk_device()
                .destroy_pipeline_layout(layout.layout, None);
        }
        Ok(())
    }
}

impl HostComputePipeline for VkContextView<'_> {
    fn drop(&mut self, rep: Resource<ComputePipeline>) -> wasmtime::anyhow::Result<()> {
        let key = Resource::<GpuComputePipeline>::new_own(rep.rep());
        let pipeline = self.table.delete(key)?;
        unsafe {
            self.vk_device().destroy_pipeline(pipeline.pipeline, None);
        }
        Ok(())
    }
}

impl HostGraphicsPipeline for VkContextView<'_> {
    fn drop(&mut self, rep: Resource<GraphicsPipeline>) -> wasmtime::anyhow::Result<()> {
        let key = Resource::<GpuGraphicsPipeline>::new_own(rep.rep());
        let pipeline = self.table.delete(key)?;
        unsafe {
            self.vk_device().destroy_pipeline(pipeline.pipeline, None);
        }
        Ok(())
    }
}

impl HostRayTracingPipeline for VkContextView<'_> {
    fn drop(&mut self, rep: Resource<RayTracingPipeline>) -> wasmtime::anyhow::Result<()> {
        let key = Resource::<GpuRayTracingPipeline>::new_own(rep.rep());
        let pipeline = self.table.delete(key)?;
        unsafe {
            self.vk_device().destroy_pipeline(pipeline.pipeline, None);
        }
        Ok(())
    }
}
