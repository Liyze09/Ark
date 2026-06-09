use std::{
    borrow::Cow,
    collections::HashMap,
    sync::{Mutex, OnceLock},
};
use vulkanalia::vk;
use vulkanalia_vma::vma::VmaAllocator;
use wasmtime::component::{HasData, ResourceTable};

pub mod binding;

pub struct VkContextOwned {
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
    pub graphics_command_pool: OnceLock<vk::CommandPool>,
    pub compute_command_pool: OnceLock<vk::CommandPool>,
    pub transfer_command_pool: OnceLock<vk::CommandPool>,
    /// Semaphores created by `Submission::flush()` (without fence).
    /// These live until context destruction; the GPU may still be using them.
    pub pending_semaphores: Mutex<Vec<vk::Semaphore>>,
}

pub struct VkContextView<'a> {
    pub owned: &'a VkContextOwned,
    pub table: &'a mut ResourceTable,
    pub files: &'a HashMap<String, Cow<'static, [u8]>>,
}

impl VkContextOwned {
    /// Create a new VkContextOwned.
    ///
    /// # Safety
    /// All Vulkan handles must be valid and created from the same device.
    /// `device_commands` must be loaded from the device using
    /// `vkGetDeviceProcAddr`.
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn new(
        instance: vk::Instance,
        device: vk::Device,
        device_commands: vk::DeviceCommands,
        vma: VmaAllocator,
        graphics_queue: vk::Queue,
        compute_queue: vk::Queue,
        transfer_queue: vk::Queue,
        graphics_queue_family_index: u32,
        compute_queue_family_index: u32,
        transfer_queue_family_index: u32,
    ) -> Self {
        Self {
            instance,
            device,
            device_commands,
            vma,
            graphics_queue,
            compute_queue,
            transfer_queue,
            graphics_queue_family_index,
            compute_queue_family_index,
            transfer_queue_family_index,
            graphics_command_pool: OnceLock::new(),
            compute_command_pool: OnceLock::new(),
            transfer_command_pool: OnceLock::new(),
            pending_semaphores: Mutex::new(Vec::new()),
        }
    }
}

impl<'a> VkContextView<'a> {
    /// Returns a pair `(&DeviceCommands, vk::Device)` that implements
    /// `DeviceV1_0`, allowing us to call Vulkan device functions.
    #[inline]
    pub fn vk_device(&self) -> (&vk::DeviceCommands, vk::Device) {
        (&self.owned.device_commands, self.owned.device)
    }
}

impl<'a> std::ops::Deref for VkContextView<'a> {
    type Target = VkContextOwned;
    fn deref(&self) -> &Self::Target {
        self.owned
    }
}

impl Drop for VkContextOwned {
    fn drop(&mut self) {
        unsafe {
            for sem in self.pending_semaphores.lock().unwrap().drain(..) {
                (self.device_commands.destroy_semaphore)(self.device, sem, std::ptr::null());
            }
            for pool in [
                self.graphics_command_pool.get(),
                self.compute_command_pool.get(),
                self.transfer_command_pool.get(),
            ]
            .into_iter()
            .flatten()
            {
                (self.device_commands.destroy_command_pool)(self.device, *pool, std::ptr::null());
            }
        }
    }
}

unsafe impl Send for VkContextOwned {}
unsafe impl Sync for VkContextOwned {}

impl HasData for VkContextOwned {
    type Data<'a> = VkContextView<'a>;
}

pub trait VkView {
    fn ctx<'a>(&'a mut self) -> VkContextView<'a>;
}

pub fn add_to_linker<T>(linker: &mut wasmtime::component::Linker<T>) -> Result<(), wasmtime::Error>
where
    T: VkView + 'static,
{
    fn get<T: VkView>(state: &mut T) -> VkContextView<'_> {
        state.ctx()
    }

    crate::binding::ark::gpu::buffer::add_to_linker::<_, VkContextOwned>(linker, get)?;
    crate::binding::ark::gpu::sync::add_to_linker::<_, VkContextOwned>(linker, get)?;
    crate::binding::ark::gpu::shader::add_to_linker::<_, VkContextOwned>(linker, get)?;
    crate::binding::ark::gpu::descriptor::add_to_linker::<_, VkContextOwned>(linker, get)?;
    crate::binding::ark::gpu::pipeline::add_to_linker::<_, VkContextOwned>(linker, get)?;
    crate::binding::ark::gpu::image::add_to_linker::<_, VkContextOwned>(linker, get)?;
    crate::binding::ark::gpu::command_buffer::add_to_linker::<_, VkContextOwned>(linker, get)?;
    crate::binding::ark::gpu::queue::add_to_linker::<_, VkContextOwned>(linker, get)?;

    Ok(())
}
