use vulkanalia::prelude::v1_0::*;
use vulkanalia::vk::{self, DeviceV1_2, HasBuilder};
use wasmtime::component::Resource;

use crate::{
    VkContextView,
    binding::{
        ark::gpu::{
            core::{QueueFamily, VulkanError},
            sync::{
                BinarySemaphore, Fence, Host, HostBinarySemaphore, HostFence,
                HostTimelineSemaphore, SharingMode, TimelineSemaphore,
            },
        },
        vk_err,
    },
};

/// Convert a WIT `PipelineStage` flags value to a `VkPipelineStageFlags2` bitmask.
pub fn vk_pipeline_stage_flags(stage: super::ark::gpu::core::PipelineStage) -> vk::PipelineStageFlags2 {
    use vk::PipelineStageFlags2 as Vk;
    let mut vk = Vk::empty();
    macro_rules! set {
        ($wit:ident => $vk:expr) => {
            if stage.contains(super::ark::gpu::core::PipelineStage::$wit) { vk |= $vk; }
        };
    }
    set!(TOP_OF_PIPE => Vk::TOP_OF_PIPE);
    set!(DRAW_INDIRECT => Vk::DRAW_INDIRECT);
    set!(VERTEX_INPUT => Vk::VERTEX_INPUT);
    set!(VERTEX_SHADER => Vk::VERTEX_SHADER);
    set!(TESSELLATION_CONTROL_SHADER => Vk::TESSELLATION_CONTROL_SHADER);
    set!(TESSELLATION_EVALUATION_SHADER => Vk::TESSELLATION_EVALUATION_SHADER);
    set!(GEOMETRY_SHADER => Vk::GEOMETRY_SHADER);
    set!(FRAGMENT_SHADER => Vk::FRAGMENT_SHADER);
    set!(EARLY_FRAGMENT_TESTS => Vk::EARLY_FRAGMENT_TESTS);
    set!(LATE_FRAGMENT_TESTS => Vk::LATE_FRAGMENT_TESTS);
    set!(COLOR_ATTACHMENT_OUTPUT => Vk::COLOR_ATTACHMENT_OUTPUT);
    set!(COMPUTE_SHADER => Vk::COMPUTE_SHADER);
    set!(TRANSFER => Vk::ALL_TRANSFER);
    set!(BOTTOM_OF_PIPE => Vk::BOTTOM_OF_PIPE);
    set!(HOST => Vk::HOST);
    set!(ALL_GRAPHICS => Vk::ALL_GRAPHICS);
    set!(ALL_COMMANDS => Vk::ALL_COMMANDS);
    set!(COPY => Vk::COPY);
    set!(RESOLVE => Vk::RESOLVE);
    set!(BLIT => Vk::BLIT);
    set!(CLEAR => Vk::CLEAR);
    set!(INDEX_INPUT => Vk::INDEX_INPUT);
    set!(VERTEX_ATTRIBUTE_INPUT => Vk::VERTEX_ATTRIBUTE_INPUT);
    set!(PRE_RASTERIZATION_SHADERS => Vk::PRE_RASTERIZATION_SHADERS);
    set!(VIDEO_DECODE_KHR => Vk::from_bits_truncate(0x04000000));
    set!(VIDEO_ENCODE_KHR => Vk::from_bits_truncate(0x08000000));
    set!(TRANSFORM_FEEDBACK_EXT => Vk::from_bits_truncate(0x01000000));
    set!(CONDITIONAL_RENDERING_EXT => Vk::from_bits_truncate(0x00040000));
    set!(COMMAND_PREPROCESS_EXT => Vk::from_bits_truncate(0x00020000));
    set!(FRAGMENT_SHADING_RATE_ATTACHMENT_KHR => Vk::from_bits_truncate(0x00400000));
    set!(ACCELERATION_STRUCTURE_BUILD_KHR => Vk::from_bits_truncate(0x02000000));
    set!(RAY_TRACING_SHADER_KHR => Vk::from_bits_truncate(0x00200000));
    set!(FRAGMENT_DENSITY_PROCESS_EXT => Vk::from_bits_truncate(0x00800000));
    set!(TASK_SHADER_EXT => Vk::from_bits_truncate(0x00080000));
    set!(MESH_SHADER_EXT => Vk::from_bits_truncate(0x00100000));
    set!(SUBPASS_SHADER_HUAWEI => Vk::from_bits_truncate(0x8000000000000000));
    set!(INVOCATION_MASK_HUAWEI => Vk::from_bits_truncate(0x2000000000000000));
    set!(ACCELERATION_STRUCTURE_COPY_KHR => Vk::from_bits_truncate(0x10000000));
    set!(MICROMAP_BUILD_EXT => Vk::from_bits_truncate(0x40000000));
    set!(CLUSTER_CULLING_SHADER_HUAWEI => Vk::from_bits_truncate(0x4000000000000000));
    set!(OPTICAL_FLOW_NV => Vk::from_bits_truncate(0x20000000));
    set!(CONVERT_COOPERATIVE_VECTOR_MATRIX_NV => Vk::from_bits_truncate(0x1000000000000000));
    set!(DATA_GRAPH_ARM => Vk::from_bits_truncate(0x800000000000000));
    set!(COPY_INDIRECT_KHR => Vk::from_bits_truncate(0x200000000000000));
    set!(MEMORY_DECOMPRESSION_EXT => Vk::from_bits_truncate(0x100000000000000));
    vk
}

pub fn vk_sharing_mode(
    ctx: &VkContextView<'_>,
    mode: &Option<SharingMode>,
) -> (vk::SharingMode, Vec<u32>) {
    match mode {
        None | Some(SharingMode::Exclusive) => (vk::SharingMode::EXCLUSIVE, vec![]),
        Some(SharingMode::Concurrent(families)) => {
            let indices: Vec<u32> = families
                .iter()
                .map(|qf| match qf {
                    QueueFamily::Graphics => ctx.graphics_queue_family_index,
                    QueueFamily::Compute => ctx.compute_queue_family_index,
                    QueueFamily::Transfer => ctx.transfer_queue_family_index,
                })
                .collect();
            (vk::SharingMode::CONCURRENT, indices)
        }
    }
}

pub(crate) struct GpuFence {
    pub(crate) fence: vk::Fence,
}

pub(crate) struct GpuBinarySemaphore {
    pub(crate) semaphore: vk::Semaphore,
}

pub(crate) struct GpuTimelineSemaphore {
    pub(crate) semaphore: vk::Semaphore,
}

// ── Interface-level functions ──

impl Host for VkContextView<'_> {
    fn wait_for_fences(&mut self,fences: Vec<Resource<Fence>>, wait_all: bool, timeout: Option<u64>) -> Result<(),VulkanError> {
        let mut vk_fences: Vec<vk::Fence> = vec![];
        for fence in fences {
            let key = Resource::<GpuFence>::new_borrow(fence.rep());
            let gpu_fence = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
            vk_fences.push(gpu_fence.fence);
        }
        unsafe {
            self.vk_device()
                .wait_for_fences(&vk_fences, wait_all, timeout.unwrap_or(u64::MAX))
                .map(|_| ())
                .map_err(vk_err)
        }
    }
}

// ── HostFence ──

impl HostFence for VkContextView<'_> {
    fn create(&mut self, signaled: bool) -> Result<Resource<Fence>, VulkanError> {
        let flags = if signaled {
            vk::FenceCreateFlags::SIGNALED
        } else {
            vk::FenceCreateFlags::empty()
        };
        let info = vk::FenceCreateInfo::builder().flags(flags);
        let fence = unsafe { self.vk_device().create_fence(&info, None) }.map_err(vk_err)?;
        let handle = self.table.push(GpuFence { fence })
            .map_err(|_| VulkanError::OutOfHostMemory)?;
        Ok(Resource::new_own(handle.rep()))
    }

    fn wait(
        &mut self,
        self_: Resource<Fence>,
        timeout_ns: Option<u64>,
    ) -> Result<(), VulkanError> {
        let key = Resource::<GpuFence>::new_borrow(self_.rep());
        let gpu_fence = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        unsafe {
            self.vk_device()
                .wait_for_fences(&[gpu_fence.fence], true, timeout_ns.unwrap_or(u64::MAX))
                .map(|_| ())
                .map_err(vk_err)
        }
    }

    fn is_signaled(&mut self, self_: Resource<Fence>) -> Result<bool, VulkanError> {
        let key = Resource::<GpuFence>::new_borrow(self_.rep());
        let gpu_fence = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        let raw = unsafe {
            (self.owned.device_commands.get_fence_status)(self.owned.device, gpu_fence.fence)
        };
        match raw {
            vk::Result::SUCCESS => Ok(true),
            vk::Result::NOT_READY => Ok(false),
            _ => Err(vk_err(vk::ErrorCode::from(raw))),
        }
    }

    fn reset(&mut self, self_: Resource<Fence>) -> Result<(), VulkanError> {
        let key = Resource::<GpuFence>::new_borrow(self_.rep());
        let gpu_fence = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        unsafe {
            self.vk_device()
                .reset_fences(&[gpu_fence.fence])
                .map_err(vk_err)
        }
    }

    fn drop(&mut self, rep: Resource<Fence>) -> wasmtime::anyhow::Result<()> {
        let key = Resource::<GpuFence>::new_own(rep.rep());
        let gpu_fence = self.table.delete(key)?;
        unsafe { self.vk_device().destroy_fence(gpu_fence.fence, None); }
        Ok(())
    }
}

// ── HostBinarySemaphore ──

impl HostBinarySemaphore for VkContextView<'_> {
    fn create(&mut self) -> Result<Resource<BinarySemaphore>, VulkanError> {
        let info = vk::SemaphoreCreateInfo::builder();
        let semaphore = unsafe { self.vk_device().create_semaphore(&info, None) }.map_err(vk_err)?;
        let handle = self.table.push(GpuBinarySemaphore { semaphore })
            .map_err(|_| VulkanError::OutOfHostMemory)?;
        Ok(Resource::new_own(handle.rep()))
    }

    fn drop(&mut self, rep: Resource<BinarySemaphore>) -> wasmtime::anyhow::Result<()> {
        let key = Resource::<GpuBinarySemaphore>::new_own(rep.rep());
        let gpu_sem = self.table.delete(key)?;
        unsafe { self.vk_device().destroy_semaphore(gpu_sem.semaphore, None); }
        Ok(())
    }
}

// ── HostTimelineSemaphore ──

impl HostTimelineSemaphore for VkContextView<'_> {
    fn create(&mut self, initial_value: u64) -> Result<Resource<TimelineSemaphore>, VulkanError> {
        let mut type_info = vk::SemaphoreTypeCreateInfo::builder()
            .semaphore_type(vk::SemaphoreType::TIMELINE)
            .initial_value(initial_value)
            .build();
        let info = vk::SemaphoreCreateInfo::builder().push_next(&mut type_info);
        let semaphore = unsafe { self.vk_device().create_semaphore(&info, None) }.map_err(vk_err)?;
        let handle = self.table.push(GpuTimelineSemaphore { semaphore })
            .map_err(|_| VulkanError::OutOfHostMemory)?;
        Ok(Resource::new_own(handle.rep()))
    }

    fn signal(&mut self, self_: Resource<TimelineSemaphore>, value: u64) -> Result<(), VulkanError> {
        let key = Resource::<GpuTimelineSemaphore>::new_borrow(self_.rep());
        let gpu_sem = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        let signal_info = vk::SemaphoreSignalInfo::builder()
            .semaphore(gpu_sem.semaphore)
            .value(value);
        unsafe { self.vk_device().signal_semaphore(&signal_info) }.map_err(vk_err)
    }

    fn wait(
        &mut self,
        semaphores: Vec<Resource<TimelineSemaphore>>,
        values: Vec<u64>,
        wait_all: bool,
        timeout_ns: u64,
    ) -> Result<(), VulkanError> {
        let mut vk_sems: Vec<vk::Semaphore> = Vec::with_capacity(semaphores.len());
        let mut vk_values: Vec<u64> = Vec::with_capacity(semaphores.len());
        for (i, sem) in semaphores.iter().enumerate() {
            let key = Resource::<GpuTimelineSemaphore>::new_borrow(sem.rep());
            let gpu_sem = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
            vk_sems.push(gpu_sem.semaphore);
            vk_values.push(values[i]);
        }
        let wait_info = vk::SemaphoreWaitInfo::builder()
            .semaphores(&vk_sems)
            .values(&vk_values)
            .flags(if wait_all { vk::SemaphoreWaitFlags::empty() } else { vk::SemaphoreWaitFlags::ANY });
        unsafe { self.vk_device().wait_semaphores(&wait_info, timeout_ns) }.map_err(vk_err)?;
        Ok(())
    }

    fn drop(&mut self, rep: Resource<TimelineSemaphore>) -> wasmtime::anyhow::Result<()> {
        let key = Resource::<GpuTimelineSemaphore>::new_own(rep.rep());
        let gpu_sem = self.table.delete(key)?;
        unsafe { self.vk_device().destroy_semaphore(gpu_sem.semaphore, None); }
        Ok(())
    }
}
