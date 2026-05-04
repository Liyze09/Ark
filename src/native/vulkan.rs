use vulkanalia::{vk, Entry};
use vulkanalia_vma::vma::VmaAllocator;

pub struct VkBackend {
    pub entry: Entry,
    pub instance: vk::Instance,
    pub device: vk::Device,
    pub vma: VmaAllocator,
    pub compute_queue: vk::Queue,
    pub graphics_queue: vk::Queue,
    pub transfer_queue: vk::Queue,
}

impl VkBackend {}
