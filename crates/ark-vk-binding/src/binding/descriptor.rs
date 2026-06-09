use vulkanalia::prelude::v1_0::*;
use vulkanalia::vk::{self, HasBuilder};
use wasmtime::component::Resource;

use crate::{
    VkContextView,
    binding::{
        ark::gpu::{
            core::VulkanError,
            descriptor::{
                DescriptorBinding, DescriptorBindingFlags, DescriptorPool,
                DescriptorPoolCreateFlags, DescriptorSet, DescriptorSetLayout, DescriptorType,
                DescriptorWrite, Host, HostDescriptorPool, HostDescriptorSet,
                HostDescriptorSetLayout, PoolSize,
            },
        },
        vk_err,
    },
};

// ── Type helpers ──

fn vk_descriptor_type(ty: DescriptorType) -> vk::DescriptorType {
    match ty {
        DescriptorType::Sampler => vk::DescriptorType::SAMPLER,
        DescriptorType::CombinedImageSampler => vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        DescriptorType::SampledImage => vk::DescriptorType::SAMPLED_IMAGE,
        DescriptorType::StorageImage => vk::DescriptorType::STORAGE_IMAGE,
        DescriptorType::UniformTexelBuffer => vk::DescriptorType::UNIFORM_TEXEL_BUFFER,
        DescriptorType::StorageTexelBuffer => vk::DescriptorType::STORAGE_TEXEL_BUFFER,
        DescriptorType::UniformBuffer => vk::DescriptorType::UNIFORM_BUFFER,
        DescriptorType::StorageBuffer => vk::DescriptorType::STORAGE_BUFFER,
        DescriptorType::InputAttachment => vk::DescriptorType::INPUT_ATTACHMENT,
    }
}

fn vk_binding_flags(flags: DescriptorBindingFlags) -> vk::DescriptorBindingFlags {
    let mut vkf = vk::DescriptorBindingFlags::empty();
    if flags.contains(DescriptorBindingFlags::UPDATE_AFTER_BIND) {
        vkf |= vk::DescriptorBindingFlags::UPDATE_AFTER_BIND;
    }
    if flags.contains(DescriptorBindingFlags::UPDATE_UNUSED_WHILE_PENDING) {
        vkf |= vk::DescriptorBindingFlags::UPDATE_UNUSED_WHILE_PENDING;
    }
    if flags.contains(DescriptorBindingFlags::PARTIALLY_BOUND) {
        vkf |= vk::DescriptorBindingFlags::PARTIALLY_BOUND;
    }
    if flags.contains(DescriptorBindingFlags::VARIABLE_DESCRIPTOR_COUNT) {
        vkf |= vk::DescriptorBindingFlags::VARIABLE_DESCRIPTOR_COUNT;
    }
    vkf
}

fn vk_pool_create_flags(flags: DescriptorPoolCreateFlags) -> vk::DescriptorPoolCreateFlags {
    let mut vkf = vk::DescriptorPoolCreateFlags::empty();
    if flags.contains(DescriptorPoolCreateFlags::UPDATE_AFTER_BIND) {
        vkf |= vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND;
    }
    if flags.contains(DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET) {
        vkf |= vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET;
    }
    vkf
}

// ── Internal types ──

pub(crate) struct GpuDescriptorSetLayout {
    pub(crate) layout: vk::DescriptorSetLayout,
    /// The bindings used to create this layout, kept for reference.
    _bindings: Vec<vk::DescriptorSetLayoutBinding>,
}

pub(crate) struct GpuDescriptorPool {
    pool: vk::DescriptorPool,
    /// Whether the pool was created with `FREE_DESCRIPTOR_SET`.
    freeable: bool,
}

pub(crate) struct GpuDescriptorSet {
    pub(crate) set: vk::DescriptorSet,
    /// The pool this set was allocated from (needed for `free_descriptor_sets`).
    pool: vk::DescriptorPool,
    /// Whether individual sets can be freed from this pool.
    freeable: bool,
}

// ── Descriptor Set Layout ──

fn build_vk_bindings(
    bindings: &[DescriptorBinding],
) -> (
    Vec<vk::DescriptorSetLayoutBinding>,
    Vec<vk::DescriptorBindingFlags>,
) {
    let mut vk_bindings = Vec::with_capacity(bindings.len());
    let mut vk_flags = Vec::with_capacity(bindings.len());

    for b in bindings {
        vk_bindings.push(vk::DescriptorSetLayoutBinding {
            binding: b.binding,
            descriptor_type: vk_descriptor_type(b.descriptor_type),
            descriptor_count: b.descriptor_count,
            stage_flags: vk::ShaderStageFlags::from_bits_truncate(b.stage_flags),
            immutable_samplers: std::ptr::null(),
        });
        vk_flags.push(vk_binding_flags(b.binding_flags));
    }

    (vk_bindings, vk_flags)
}

impl Host for VkContextView<'_> {
    fn create_descriptor_set_layout(
        &mut self,
        bindings: Vec<DescriptorBinding>,
    ) -> Result<Resource<DescriptorSetLayout>, VulkanError> {
        if bindings.is_empty() {
            return Err(VulkanError::Unnamed("bindings must not be empty".into()));
        }

        let (vk_bindings, vk_binding_flags) = build_vk_bindings(&bindings);

        let mut binding_flags_info = vk::DescriptorSetLayoutBindingFlagsCreateInfo::builder()
            .binding_flags(&vk_binding_flags)
            .build();

        let layout_info = vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(&vk_bindings)
            .push_next(&mut binding_flags_info);

        let layout = unsafe {
            self.vk_device()
                .create_descriptor_set_layout(&layout_info, None)
        }
        .map_err(vk_err)?;

        let gpu_layout = GpuDescriptorSetLayout {
            layout,
            _bindings: vk_bindings,
        };
        let handle = self
            .table
            .push(gpu_layout)
            .map_err(|_| VulkanError::OutOfHostMemory)?;
        Ok(Resource::new_own(handle.rep()))
    }

    fn create_descriptor_pool(
        &mut self,
        max_sets: u32,
        pool_sizes: Vec<PoolSize>,
        create_flags: DescriptorPoolCreateFlags,
    ) -> Result<Resource<DescriptorPool>, VulkanError> {
        if pool_sizes.is_empty() {
            return Err(VulkanError::Unnamed("pool sizes must not be empty".into()));
        }

        let vk_pool_sizes: Vec<vk::DescriptorPoolSize> = pool_sizes
            .iter()
            .map(|s| {
                vk::DescriptorPoolSize::builder()
                    .type_(vk_descriptor_type(s.descriptor_type))
                    .descriptor_count(s.descriptor_count)
                    .build()
            })
            .collect();

        let pool_info = vk::DescriptorPoolCreateInfo::builder()
            .pool_sizes(&vk_pool_sizes)
            .max_sets(max_sets)
            .flags(vk_pool_create_flags(create_flags));

        let pool =
            unsafe { self.vk_device().create_descriptor_pool(&pool_info, None) }.map_err(vk_err)?;

        let freeable = create_flags.contains(DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET);
        let gpu_pool = GpuDescriptorPool { pool, freeable };
        let handle = self
            .table
            .push(gpu_pool)
            .map_err(|_| VulkanError::OutOfHostMemory)?;
        Ok(Resource::new_own(handle.rep()))
    }

    fn allocate_descriptor_set(
        &mut self,
        pool: Resource<DescriptorPool>,
        layout: Resource<DescriptorSetLayout>,
        variable_descriptor_counts: Vec<u32>,
    ) -> Result<Resource<DescriptorSet>, VulkanError> {
        let pool_key = Resource::<GpuDescriptorPool>::new_borrow(pool.rep());
        let gpu_pool = self
            .table
            .get(&pool_key)
            .map_err(|_| VulkanError::Unknown)?;

        let layout_key = Resource::<GpuDescriptorSetLayout>::new_borrow(layout.rep());
        let gpu_layout = self
            .table
            .get(&layout_key)
            .map_err(|_| VulkanError::Unknown)?;

        let set_layouts = [gpu_layout.layout];

        let alloc_info = if variable_descriptor_counts.is_empty() {
            vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(gpu_pool.pool)
                .set_layouts(&set_layouts)
                .build()
        } else {
            let mut count_info = vk::DescriptorSetVariableDescriptorCountAllocateInfo::builder()
                .descriptor_counts(&variable_descriptor_counts)
                .build();
            vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(gpu_pool.pool)
                .set_layouts(&set_layouts)
                .push_next(&mut count_info)
                .build()
        };

        let sets =
            unsafe { self.vk_device().allocate_descriptor_sets(&alloc_info) }.map_err(vk_err)?;

        let gpu_set = GpuDescriptorSet {
            set: sets[0],
            pool: gpu_pool.pool,
            freeable: gpu_pool.freeable,
        };
        let handle = self
            .table
            .push(gpu_set)
            .map_err(|_| VulkanError::OutOfHostMemory)?;
        Ok(Resource::new_own(handle.rep()))
    }

    fn write_descriptor_set(
        &mut self,
        set: Resource<DescriptorSet>,
        writes: Vec<DescriptorWrite>,
    ) -> Result<(), VulkanError> {
        let set_key = Resource::<GpuDescriptorSet>::new_borrow(set.rep());
        let gpu_set = self.table.get(&set_key).map_err(|_| VulkanError::Unknown)?;

        let mut buffer_infos: Vec<vk::DescriptorBufferInfo> = Vec::new();
        let mut image_infos: Vec<vk::DescriptorImageInfo> = Vec::new();
        let mut write_descs: Vec<vk::WriteDescriptorSet> = Vec::new();

        for write in &writes {
            let desc_type = vk_descriptor_type(write.descriptor_type);

            let mut write_desc = vk::WriteDescriptorSet::builder()
                .dst_set(gpu_set.set)
                .dst_binding(write.binding)
                .dst_array_element(write.dst_array_element)
                .descriptor_type(desc_type);

            if let Some(ref buf_info) = write.buffer_info {
                let buf_key =
                    Resource::<super::buffer::GpuBuffer>::new_borrow(buf_info.buffer.rep());
                let buf = self
                    .table
                    .get(&buf_key)
                    .map_err(|_| VulkanError::InvalidBuffer)?;
                buffer_infos.push(
                    vk::DescriptorBufferInfo::builder()
                        .buffer(buf.buffer)
                        .offset(buf_info.offset)
                        .range(buf_info.range)
                        .build(),
                );
                write_desc = write_desc
                    .buffer_info(std::slice::from_ref(&buffer_infos[buffer_infos.len() - 1]));
            }

            if let Some(ref img_info) = write.image_info {
                let view_key =
                    Resource::<super::image::GpuImageView>::new_borrow(img_info.image_view.rep());
                let view = self
                    .table
                    .get(&view_key)
                    .map_err(|_| VulkanError::Unknown)?;

                let sampler_vk = if let Some(ref sampler_res) = img_info.sampler {
                    let sampler_key =
                        Resource::<super::image::GpuSampler>::new_borrow(sampler_res.rep());
                    let sampler = self
                        .table
                        .get(&sampler_key)
                        .map_err(|_| VulkanError::Unknown)?;
                    sampler.sampler
                } else {
                    vk::Sampler::default()
                };

                image_infos.push(
                    vk::DescriptorImageInfo::builder()
                        .image_layout(vk::ImageLayout::from_raw(img_info.image_layout as i32))
                        .image_view(view.view)
                        .sampler(sampler_vk)
                        .build(),
                );
                write_desc = write_desc
                    .image_info(std::slice::from_ref(&image_infos[image_infos.len() - 1]));
            }

            write_descs.push(write_desc.build());
        }

        unsafe {
            self.vk_device()
                .update_descriptor_sets(&write_descs, &[] as &[vk::CopyDescriptorSet]);
        }

        Ok(())
    }
}

// ── Drop impls ──

impl HostDescriptorSetLayout for VkContextView<'_> {
    fn drop(&mut self, rep: Resource<DescriptorSetLayout>) -> wasmtime::anyhow::Result<()> {
        let key = Resource::<GpuDescriptorSetLayout>::new_own(rep.rep());
        let layout = self.table.delete(key)?;
        unsafe {
            self.vk_device()
                .destroy_descriptor_set_layout(layout.layout, None);
        }
        Ok(())
    }
}

impl HostDescriptorPool for VkContextView<'_> {
    fn drop(&mut self, rep: Resource<DescriptorPool>) -> wasmtime::anyhow::Result<()> {
        let key = Resource::<GpuDescriptorPool>::new_own(rep.rep());
        let pool = self.table.delete(key)?;
        unsafe {
            self.vk_device().destroy_descriptor_pool(pool.pool, None);
        }
        Ok(())
    }
}

impl HostDescriptorSet for VkContextView<'_> {
    fn drop(&mut self, rep: Resource<DescriptorSet>) -> wasmtime::anyhow::Result<()> {
        let key = Resource::<GpuDescriptorSet>::new_own(rep.rep());
        let set = self.table.delete(key)?;
        if set.freeable {
            // Errors here are benign (the pool will free everything on
            // destruction anyway).
            let _ = unsafe { self.vk_device().free_descriptor_sets(set.pool, &[set.set]) };
        }
        Ok(())
    }
}
