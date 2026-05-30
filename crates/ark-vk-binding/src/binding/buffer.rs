use vulkanalia::vk::{self, HasBuilder};
use vulkanalia_vma::Alloc;
use wasmtime::component::Resource;

use crate::{
    VkContextView,
    binding::{ark::{self, gpu::{
        buffer::{Buffer, BufferCreateInfo, BufferUsage}, core::VulkanError, memory::AllocateInfo, 
    }}, memory::vma_alloc_options, sync::vk_sharing_mode, vk_err},
};

struct GpuBuffer {
    buffer: vk::Buffer,
    allocation: vulkanalia_vma::Allocation,
    size: u64,
}

fn vk_buffer_usage(usage: BufferUsage) -> vk::BufferUsageFlags {
    let mut flags = vk::BufferUsageFlags::empty();
    if usage.contains(BufferUsage::TRANSFER_SRC) { flags |= vk::BufferUsageFlags::TRANSFER_SRC; }
    if usage.contains(BufferUsage::TRANSFER_DST) { flags |= vk::BufferUsageFlags::TRANSFER_DST; }
    if usage.contains(BufferUsage::UNIFORM_TEXEL_BUFFER) { flags |= vk::BufferUsageFlags::UNIFORM_TEXEL_BUFFER; }
    if usage.contains(BufferUsage::STORAGE_TEXEL_BUFFER) { flags |= vk::BufferUsageFlags::STORAGE_TEXEL_BUFFER; }
    if usage.contains(BufferUsage::UNIFORM_BUFFER) { flags |= vk::BufferUsageFlags::UNIFORM_BUFFER; }
    if usage.contains(BufferUsage::STORAGE_BUFFER) { flags |= vk::BufferUsageFlags::STORAGE_BUFFER; }
    if usage.contains(BufferUsage::INDEX_BUFFER) { flags |= vk::BufferUsageFlags::INDEX_BUFFER; }
    if usage.contains(BufferUsage::VERTEX_BUFFER) { flags |= vk::BufferUsageFlags::VERTEX_BUFFER; }
    if usage.contains(BufferUsage::INDIRECT_BUFFER) { flags |= vk::BufferUsageFlags::INDIRECT_BUFFER; }
    if usage.contains(BufferUsage::CONDITIONAL_RENDERING_EXT) { flags |= vk::BufferUsageFlags::CONDITIONAL_RENDERING_EXT; }
    if usage.contains(BufferUsage::SHADER_BINDING_TABLE_KHR) { flags |= vk::BufferUsageFlags::SHADER_BINDING_TABLE_KHR; }
    if usage.contains(BufferUsage::TRANSFORM_FEEDBACK_BUFFER_EXT) { flags |= vk::BufferUsageFlags::TRANSFORM_FEEDBACK_BUFFER_EXT; }
    if usage.contains(BufferUsage::TRANSFORM_FEEDBACK_COUNTER_BUFFER_EXT) { flags |= vk::BufferUsageFlags::TRANSFORM_FEEDBACK_COUNTER_BUFFER_EXT; }
    if usage.contains(BufferUsage::VIDEO_DECODE_SRC_KHR) { flags |= vk::BufferUsageFlags::VIDEO_DECODE_SRC_KHR; }
    if usage.contains(BufferUsage::VIDEO_DECODE_DST_KHR) { flags |= vk::BufferUsageFlags::VIDEO_DECODE_DST_KHR; }
    if usage.contains(BufferUsage::VIDEO_ENCODE_DST_KHR) { flags |= vk::BufferUsageFlags::VIDEO_ENCODE_DST_KHR; }
    if usage.contains(BufferUsage::VIDEO_ENCODE_SRC_KHR) { flags |= vk::BufferUsageFlags::VIDEO_ENCODE_SRC_KHR; }
    if usage.contains(BufferUsage::SHADER_DEVICE_ADDRESS) { flags |= vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS; }
    if usage.contains(BufferUsage::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR) { flags |= vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR; }
    if usage.contains(BufferUsage::ACCELERATION_STRUCTURE_STORAGE_KHR) { flags |= vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR; }
    if usage.contains(BufferUsage::SAMPLER_DESCRIPTOR_BUFFER_EXT) { flags |= vk::BufferUsageFlags::SAMPLER_DESCRIPTOR_BUFFER_EXT; }
    if usage.contains(BufferUsage::RESOURCE_DESCRIPTOR_BUFFER_EXT) { flags |= vk::BufferUsageFlags::RESOURCE_DESCRIPTOR_BUFFER_EXT; }
    if usage.contains(BufferUsage::MICROMAP_BUILD_INPUT_READ_ONLY_EXT) { flags |= vk::BufferUsageFlags::MICROMAP_BUILD_INPUT_READ_ONLY_EXT; }
    if usage.contains(BufferUsage::MICROMAP_STORAGE_EXT) { flags |= vk::BufferUsageFlags::MICROMAP_STORAGE_EXT; }
    if usage.contains(BufferUsage::EXECUTION_GRAPH_SCRATCH_AMDX) { flags |= vk::BufferUsageFlags::EXECUTION_GRAPH_SCRATCH_AMDX; }
    if usage.contains(BufferUsage::PUSH_DESCRIPTORS_DESCRIPTOR_BUFFER_EXT) { flags |= vk::BufferUsageFlags::PUSH_DESCRIPTORS_DESCRIPTOR_BUFFER_EXT; }
    if usage.contains(BufferUsage::TILE_MEMORY_QCOM) { flags |= vk::BufferUsageFlags::TILE_MEMORY_QCOM; }
    if usage.contains(BufferUsage::DESCRIPTOR_HEAP_EXT) { flags |= vk::BufferUsageFlags::DESCRIPTOR_HEAP_EXT; }
    flags
}

impl ark::gpu::buffer::Host for VkContextView<'_> {
    fn buffer_from_data(
        &mut self,
        create_info: BufferCreateInfo,
        allocate_info: AllocateInfo,
        data: Vec<u8>,
    ) -> Result<Resource<Buffer>, VulkanError> {
        let (sharing_mode, queue_indices) = vk_sharing_mode(&create_info.sharing_mode);
        let usage = vk_buffer_usage(create_info.usage);

        let buffer_info = vk::BufferCreateInfo::builder()
            .size(create_info.size)
            .usage(usage)
            .sharing_mode(sharing_mode)
            .queue_family_indices(&queue_indices);
        let alloc_options = vma_alloc_options(&allocate_info, true);

        let (buffer, allocation) = unsafe { self.vma().create_buffer(buffer_info, &alloc_options) }
            .map_err(vk_err)?;

        // Upload data via mapping
        if !data.is_empty() {
            unsafe {
                let data_ptr = self.vma().map_memory(allocation).map_err(vk_err)?;
                std::ptr::copy_nonoverlapping(data.as_ptr(), data_ptr, data.len());
                self.vma().unmap_memory(allocation);
            }
        }

        let gpu_buf = GpuBuffer {
            buffer,
            allocation,
            size: create_info.size,
        };

        let handle = self.table.push(gpu_buf)
            .map_err(|_| VulkanError::OutOfHostMemory)?;

        Ok(Resource::<Buffer>::new_own(handle.rep()))
    }

    fn buffer_zeroed(
        &mut self,
        create_info: BufferCreateInfo,
        allocate_info: AllocateInfo,
    ) -> Result<Resource<Buffer>, VulkanError> {
        let (sharing_mode, queue_indices) = vk_sharing_mode(&create_info.sharing_mode);
        let usage = vk_buffer_usage(create_info.usage);

        let buffer_info = vk::BufferCreateInfo::builder()
            .size(create_info.size)
            .usage(usage)
            .sharing_mode(sharing_mode)
            .queue_family_indices(&queue_indices);
        let alloc_options = vma_alloc_options(&allocate_info, false);

        let (buffer, allocation) = unsafe { self.vma().create_buffer(buffer_info, &alloc_options) }
            .map_err(vk_err)?;

        let gpu_buf = GpuBuffer {
            buffer,
            allocation,
            size: create_info.size,
        };

        let handle = self.table.push(gpu_buf)
            .map_err(|_| VulkanError::OutOfHostMemory)?;

        Ok(Resource::<Buffer>::new_own(handle.rep()))
    }
}

impl ark::gpu::buffer::HostBuffer for VkContextView<'_> {
    fn read(
        &mut self,
        self_: Resource<Buffer>,
        start: u64,
        len: u64,
    ) -> Result<Vec<u8>, VulkanError> {
        let rep = self_.rep();
        let key = Resource::<GpuBuffer>::new_own(rep);

        let (allocation, size) = {
            let buf = self.table.get(&key)
                .map_err(|_| VulkanError::InvalidBuffer)?;
            (buf.allocation, buf.size)
        };

        // Bounds check
        if start + len > size {
            return Err(VulkanError::OutOfBounds);
        }

        if len == 0 {
            return Ok(Vec::new());
        }

        unsafe {
            let data_ptr = self.vma().map_memory(allocation).map_err(vk_err)?;
            let result = std::slice::from_raw_parts(data_ptr.add(start as usize), len as usize).to_vec();
            self.vma().unmap_memory(allocation);
            Ok(result)
        }
    }

    fn write(
        &mut self,
        self_: Resource<Buffer>,
        start: u64,
        data: Vec<u8>,
    ) -> Result<(), VulkanError> {
        let rep = self_.rep();
        let key = Resource::<GpuBuffer>::new_own(rep);

        let (allocation, size) = {
            let buf = self.table.get(&key)
                .map_err(|_| VulkanError::InvalidBuffer)?;
            (buf.allocation, buf.size)
        };

        // Bounds check
        if start + data.len() as u64 > size {
            return Err(VulkanError::OutOfBounds);
        }

        if data.is_empty() {
            return Ok(());
        }

        unsafe {
            let data_ptr = self.vma().map_memory(allocation).map_err(vk_err)?;
            std::ptr::copy_nonoverlapping(data.as_ptr(), data_ptr.add(start as usize), data.len());
            self.vma().unmap_memory(allocation);
        }

        Ok(())
    }

    fn drop(&mut self, rep: Resource<Buffer>) -> wasmtime::anyhow::Result<()> {
        let rep_u32 = rep.rep();
        let key = Resource::<GpuBuffer>::new_own(rep_u32);

        let buf = self.table.delete(key)?;
        unsafe {
            self.vma().destroy_buffer(buf.buffer, buf.allocation);
        }
        Ok(())
    }
}
