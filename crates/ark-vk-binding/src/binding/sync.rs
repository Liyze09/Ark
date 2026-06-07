use vulkanalia::prelude::v1_0::*;
use vulkanalia::vk::{self, HasBuilder};
use wasmtime::component::Resource;

use crate::{
    binding::{
        ark::gpu::{
            core::{QueueFamily, VulkanError},
            sync::{Fence, Host, HostFence, HostSemaphore, Semaphore, SharingMode},
        },
        vk_err,
    },
    VkContextView,
};

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

pub(crate) struct GpuSemaphore {
    pub(crate) semaphore: vk::Semaphore,
}

impl Host for VkContextView<'_> {
    fn create_fence(&mut self, signaled: bool) -> Result<Resource<Fence>, VulkanError> {
        let flags = if signaled {
            vk::FenceCreateFlags::SIGNALED
        } else {
            vk::FenceCreateFlags::empty()
        };
        let info = vk::FenceCreateInfo::builder().flags(flags);
        let fence = unsafe { self.vk_device().create_fence(&info, None) }.map_err(vk_err)?;
        let handle = self
            .table
            .push(GpuFence { fence })
            .map_err(|_| VulkanError::OutOfHostMemory)?;
        Ok(Resource::new_own(handle.rep()))
    }

    fn wait_for_fence(
        &mut self,
        fence: Resource<Fence>,
        timeout_ns: u64,
    ) -> Result<(), VulkanError> {
        let key = Resource::<GpuFence>::new_borrow(fence.rep());
        let gpu_fence = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        unsafe {
            self.vk_device()
                .wait_for_fences(&[gpu_fence.fence], true, timeout_ns)
                .map(|_| ())
                .map_err(vk_err)
        }
    }

    fn fence_is_signaled(&mut self, fence: Resource<Fence>) -> Result<bool, VulkanError> {
        let key = Resource::<GpuFence>::new_borrow(fence.rep());
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

    fn reset_fence(&mut self, fence: Resource<Fence>) -> Result<(), VulkanError> {
        let key = Resource::<GpuFence>::new_borrow(fence.rep());
        let gpu_fence = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        unsafe {
            self.vk_device()
                .reset_fences(&[gpu_fence.fence])
                .map_err(vk_err)
        }
    }

    fn create_semaphore(&mut self) -> Result<Resource<Semaphore>, VulkanError> {
        let info = vk::SemaphoreCreateInfo::builder();
        let semaphore =
            unsafe { self.vk_device().create_semaphore(&info, None) }.map_err(vk_err)?;
        let handle = self
            .table
            .push(GpuSemaphore { semaphore })
            .map_err(|_| VulkanError::OutOfHostMemory)?;
        Ok(Resource::new_own(handle.rep()))
    }
}

impl HostFence for VkContextView<'_> {
    fn drop(&mut self, rep: Resource<Fence>) -> wasmtime::anyhow::Result<()> {
        let key = Resource::<GpuFence>::new_own(rep.rep());
        let gpu_fence = self.table.delete(key)?;
        unsafe {
            self.vk_device().destroy_fence(gpu_fence.fence, None);
        }
        Ok(())
    }
}

impl HostSemaphore for VkContextView<'_> {
    fn drop(&mut self, rep: Resource<Semaphore>) -> wasmtime::anyhow::Result<()> {
        let key = Resource::<GpuSemaphore>::new_own(rep.rep());
        let gpu_sem = self.table.delete(key)?;
        unsafe {
            self.vk_device().destroy_semaphore(gpu_sem.semaphore, None);
        }
        Ok(())
    }
}
