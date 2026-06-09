use vulkanalia::vk;
use vulkanalia_vma::vma::VmaAllocator;

#[derive(Clone)]
pub struct VkBackend {
    pub instance: vk::Instance,
    pub device: vk::Device,
    pub device_commands: vk::DeviceCommands,
    pub vma: VmaAllocator,
    pub graphics_queue: vk::Queue,
    pub compute_queue: vk::Queue,
    pub transfer_queue: vk::Queue,
    pub graphics_queue_family_index: u32,
    pub compute_queue_family_index: u32,
    pub transfer_queue_family_index: u32,
}

impl VkBackend {
    /// # Safety
    /// All Vulkan handles must be valid.
    pub unsafe fn to_vk_context(&self) -> ark_vk_binding::VkContextOwned {
        unsafe { ark_vk_binding::VkContextOwned::new(
            self.instance,
            self.device,
            self.device_commands,
            self.vma,
            self.graphics_queue,
            self.compute_queue,
            self.transfer_queue,
            self.graphics_queue_family_index,
            self.compute_queue_family_index,
            self.transfer_queue_family_index,
        )}
    }
}
