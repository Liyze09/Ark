use vulkanalia::{vk, Entry};
use vulkanalia::loader::{LibloadingLoader, Loader, LIBRARY};
use vulkanalia_vma::vma::VmaAllocator;

#[derive(Debug, Clone)]
pub struct VkBackend {
    pub entry: Entry,
    pub instance: vk::Instance,
    pub device: vk::Device,
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
    /// # Safety
    /// All Vulkan handles must be valid.
    pub unsafe fn to_vk_context(&self) -> ark_vk_binding::VkContextOwned {
        // Load device commands using the Vulkan loader.
        let loader = unsafe { LibloadingLoader::new(LIBRARY) }
            .expect("failed to create Vulkan loader for device commands");
        let get_instance_proc_addr: vk::PFN_vkGetInstanceProcAddr = unsafe {
            std::mem::transmute(
                loader.load(b"vkGetInstanceProcAddr\0")
                    .expect("vkGetInstanceProcAddr not found")
            )
        };
        let get_device_proc_addr: vk::PFN_vkGetDeviceProcAddr = unsafe {
            std::mem::transmute(get_instance_proc_addr(self.instance, c"vkGetDeviceProcAddr".as_ptr()))
        };
        let device_commands = unsafe {
            vk::DeviceCommands::load(|name| get_device_proc_addr(self.device, name))
        };

        unsafe { ark_vk_binding::VkContextOwned::new(
            self.entry.clone(),
            self.instance,
            self.device,
            device_commands,
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
