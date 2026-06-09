use vulkanalia::vk;
use vulkanalia_vma::{AllocationCreateFlags, AllocationOptions, MemoryUsage};

use crate::{
    VkContextView,
    binding::ark::gpu::memory::{AllocateInfo, MemoryType},
};

impl VkContextView<'_> {
    #[inline]
    pub const fn vma(&self) -> &vulkanalia_vma::Allocator {
        unsafe {
            &*(&self.owned.vma as *const vulkanalia_vma::vma::VmaAllocator
                as *const vulkanalia_vma::Allocator)
        }
    }
}

pub fn vma_alloc_options(info: &AllocateInfo, require_host_visible: bool) -> AllocationOptions {
    let mt = info.memory_type;
    let mut flags = AllocationCreateFlags::empty();

    let usage = if mt.contains(MemoryType::PREFER_DEVICE) {
        MemoryUsage::AutoPreferDevice
    } else if mt.contains(MemoryType::PREFER_HOST) {
        MemoryUsage::AutoPreferHost
    } else {
        MemoryUsage::Auto
    };

    let mut required_flags = vk::MemoryPropertyFlags::empty();
    let mut preferred_flags = vk::MemoryPropertyFlags::empty();

    if require_host_visible {
        required_flags |= vk::MemoryPropertyFlags::HOST_VISIBLE;
    }

    if mt.contains(MemoryType::HOST_SEQUENTIAL_WRITE) {
        flags |= AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE;
        preferred_flags |= vk::MemoryPropertyFlags::HOST_COHERENT;
    }
    if mt.contains(MemoryType::HOST_RANDOM_ACCESS) {
        flags |= AllocationCreateFlags::HOST_ACCESS_RANDOM;
        preferred_flags |= vk::MemoryPropertyFlags::HOST_CACHED;
    }

    AllocationOptions {
        usage,
        flags,
        required_flags,
        preferred_flags,
        ..Default::default()
    }
}
