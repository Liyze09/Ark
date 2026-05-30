use vulkanalia::{Entry, vk};
use vulkanalia_vma::vma::VmaAllocator;
use wasmtime::component::{HasData, ResourceTable};

mod binding;

pub struct VkContextOwned {
    pub entry: Entry,
    pub instance: vk::Instance,
    pub device: vk::Device,
    pub vma: VmaAllocator,
    pub compute_queue: vk::Queue,
    pub graphics_queue: vk::Queue,
    pub transfer_queue: vk::Queue,
}

pub struct VkContextView<'a> {
    pub owned: &'a VkContextOwned,
    pub table: &'a mut ResourceTable,
}

impl<'a> std::ops::Deref for VkContextView<'a> {
    type Target = VkContextOwned;
    fn deref(&self) -> &Self::Target {
        self.owned
    }
}
impl HasData for VkContextOwned {
    type Data<'a> = VkContextView<'a>;
}

pub trait VkView {
    fn ctx(&mut self) -> VkContextView<'_>;
}

pub fn add_to_linker<T>(linker: &mut wasmtime::component::Linker<T>) -> Result<(), wasmtime::Error>
where
    T: VkView + 'static,
{
    crate::binding::ark::gpu::buffer::add_to_linker::<_, VkContextOwned>(
        linker,
        |state: &mut T| state.ctx(), 
    )?;
    Ok(())
}