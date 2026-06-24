use vulkanalia::prelude::v1_0::*;
use vulkanalia::vk::{self, HasBuilder};
use vulkanalia_vma::Alloc;
use wasmtime::component::Resource;

use crate::{
    VkContextView,
    binding::{
        ark::gpu::{
            core::VulkanError,
            image::{
                BorderColor, CompareOp, Filter, Host, HostImage, HostImageView, HostSampler,
                Image, ImageAspectFlags, ImageCreateFlags, ImageCreateInfo, ImageTiling, ImageType,
                ImageUsage, ImageView, ImageViewCreateInfo, ImageViewType, SampleCount, Sampler,
                SamplerAddressMode, SamplerCreateInfo, SamplerMipmapMode,
            },
            memory::AllocateInfo,
        },
        memory::vma_alloc_options,
        vk_err,
    },
};

impl Host for VkContextView<'_> {}

pub(crate) struct GpuImage {
    pub(crate) image: vk::Image,
    allocation: vulkanalia_vma::Allocation,
}

#[repr(transparent)]
pub(crate) struct GpuImageView {
    pub(crate) view: vk::ImageView,
}

#[repr(transparent)]
pub(crate) struct GpuSampler {
    pub(crate) sampler: vk::Sampler,
}

fn vk_image_type(ty: ImageType) -> vk::ImageType {
    match ty {
        ImageType::Dim1d | ImageType::Dim1Array => vk::ImageType::_1D,
        ImageType::Dim2d | ImageType::Dim2dArray | ImageType::Cube | ImageType::CubeArray => vk::ImageType::_2D,
        ImageType::Dim3d => vk::ImageType::_3D,
    }
}

fn vk_image_view_type(ty: ImageViewType) -> vk::ImageViewType {
    match ty {
        ImageViewType::Dim1d => vk::ImageViewType::_1D,
        ImageViewType::Dim2d => vk::ImageViewType::_2D,
        ImageViewType::Dim3d => vk::ImageViewType::_3D,
        ImageViewType::Cube => vk::ImageViewType::CUBE,
        ImageViewType::Dim1dArray => vk::ImageViewType::_1D_ARRAY,
        ImageViewType::Dim2dArray => vk::ImageViewType::_2D_ARRAY,
        ImageViewType::CubeArray => vk::ImageViewType::CUBE_ARRAY,
    }
}

fn vk_image_usage(usage: ImageUsage) -> vk::ImageUsageFlags {
    let mut flags = vk::ImageUsageFlags::empty();
    if usage.contains(ImageUsage::TRANSFER_SRC) { flags |= vk::ImageUsageFlags::TRANSFER_SRC; }
    if usage.contains(ImageUsage::TRANSFER_DST) { flags |= vk::ImageUsageFlags::TRANSFER_DST; }
    if usage.contains(ImageUsage::SAMPLED) { flags |= vk::ImageUsageFlags::SAMPLED; }
    if usage.contains(ImageUsage::STORAGE) { flags |= vk::ImageUsageFlags::STORAGE; }
    if usage.contains(ImageUsage::COLOR_ATTACHMENT) { flags |= vk::ImageUsageFlags::COLOR_ATTACHMENT; }
    if usage.contains(ImageUsage::DEPTH_STENCIL_ATTACHMENT) { flags |= vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT; }
    if usage.contains(ImageUsage::TRANSIENT_ATTACHMENT) { flags |= vk::ImageUsageFlags::TRANSIENT_ATTACHMENT; }
    if usage.contains(ImageUsage::INPUT_ATTACHMENT) { flags |= vk::ImageUsageFlags::INPUT_ATTACHMENT; }
    if usage.contains(ImageUsage::SHADING_RATE_ATTACHMENT) { flags |= vk::ImageUsageFlags::FRAGMENT_SHADING_RATE_ATTACHMENT_KHR; }
    if usage.contains(ImageUsage::DENSITY_MAP_ATTACHMENT) { flags |= vk::ImageUsageFlags::FRAGMENT_DENSITY_MAP_EXT; }
    flags
}

fn vk_image_create_flags(flags: ImageCreateFlags) -> vk::ImageCreateFlags {
    let mut vkf = vk::ImageCreateFlags::empty();
    if flags.contains(ImageCreateFlags::MUTABLE_FORMAT) { vkf |= vk::ImageCreateFlags::MUTABLE_FORMAT; }
    if flags.contains(ImageCreateFlags::CUBE_COMPATIBLE) { vkf |= vk::ImageCreateFlags::CUBE_COMPATIBLE; }
    if flags.contains(ImageCreateFlags::ALIAS) { vkf |= vk::ImageCreateFlags::ALIAS; }
    if flags.contains(ImageCreateFlags::BLOCK_TEXEL_VIEW_COMPATIBLE) { vkf |= vk::ImageCreateFlags::BLOCK_TEXEL_VIEW_COMPATIBLE; }
    if flags.contains(ImageCreateFlags::EXTENDED_USAGE) { vkf |= vk::ImageCreateFlags::EXTENDED_USAGE; }
    if flags.contains(ImageCreateFlags::CORNER_SAMPLED) { vkf |= vk::ImageCreateFlags::CORNER_SAMPLED_NV; }
    vkf
}

fn vk_sample_count(samples: SampleCount) -> vk::SampleCountFlags {
    match samples {
        SampleCount::Sample1 => vk::SampleCountFlags::_1,
        SampleCount::Sample2 => vk::SampleCountFlags::_2,
        SampleCount::Sample4 => vk::SampleCountFlags::_4,
        SampleCount::Sample8 => vk::SampleCountFlags::_8,
        SampleCount::Sample16 => vk::SampleCountFlags::_16,
        SampleCount::Sample32 => vk::SampleCountFlags::_32,
        SampleCount::Sample64 => vk::SampleCountFlags::_64,
    }
}

fn vk_image_tiling(tiling: ImageTiling) -> vk::ImageTiling {
    match tiling {
        ImageTiling::Optimal => vk::ImageTiling::OPTIMAL,
        ImageTiling::Linear => vk::ImageTiling::LINEAR,
    }
}

fn vk_filter(filter: Filter) -> vk::Filter {
    match filter {
        Filter::Nearest => vk::Filter::NEAREST,
        Filter::Linear => vk::Filter::LINEAR,
    }
}

fn vk_sampler_mipmap_mode(mode: SamplerMipmapMode) -> vk::SamplerMipmapMode {
    match mode {
        SamplerMipmapMode::Nearest => vk::SamplerMipmapMode::NEAREST,
        SamplerMipmapMode::Linear => vk::SamplerMipmapMode::LINEAR,
    }
}

fn vk_sampler_address_mode(mode: SamplerAddressMode) -> vk::SamplerAddressMode {
    match mode {
        SamplerAddressMode::Repeat => vk::SamplerAddressMode::REPEAT,
        SamplerAddressMode::MirroredRepeat => vk::SamplerAddressMode::MIRRORED_REPEAT,
        SamplerAddressMode::ClampToEdge => vk::SamplerAddressMode::CLAMP_TO_EDGE,
        SamplerAddressMode::ClampToBorder => vk::SamplerAddressMode::CLAMP_TO_BORDER,
        SamplerAddressMode::MirrorClampToEdge => vk::SamplerAddressMode::MIRROR_CLAMP_TO_EDGE,
    }
}

fn vk_compare_op(op: CompareOp) -> vk::CompareOp {
    match op {
        CompareOp::Never => vk::CompareOp::NEVER,
        CompareOp::Less => vk::CompareOp::LESS,
        CompareOp::Equal => vk::CompareOp::EQUAL,
        CompareOp::LessOrEqual => vk::CompareOp::LESS_OR_EQUAL,
        CompareOp::Greater => vk::CompareOp::GREATER,
        CompareOp::NotEqual => vk::CompareOp::NOT_EQUAL,
        CompareOp::GreaterOrEqual => vk::CompareOp::GREATER_OR_EQUAL,
        CompareOp::Always => vk::CompareOp::ALWAYS,
    }
}

fn vk_border_color(color: BorderColor) -> vk::BorderColor {
    match color {
        BorderColor::FloatTransparentBlack => vk::BorderColor::FLOAT_TRANSPARENT_BLACK,
        BorderColor::IntTransparentBlack => vk::BorderColor::INT_TRANSPARENT_BLACK,
        BorderColor::FloatOpaqueBlack => vk::BorderColor::FLOAT_OPAQUE_BLACK,
        BorderColor::IntOpaqueBlack => vk::BorderColor::INT_OPAQUE_BLACK,
        BorderColor::FloatOpaqueWhite => vk::BorderColor::FLOAT_OPAQUE_WHITE,
        BorderColor::IntOpaqueWhite => vk::BorderColor::INT_OPAQUE_WHITE,
    }
}

// ── HostImage ──

impl HostImage for VkContextView<'_> {
    fn create(
        &mut self,
        create_info: ImageCreateInfo,
        alloc: AllocateInfo,
    ) -> Result<Resource<Image>, VulkanError> {
        let image_type = vk_image_type(create_info.image_type);
        let extent = vk::Extent3D::builder()
            .width(create_info.extent.width)
            .height(create_info.extent.height)
            .depth(create_info.extent.depth)
            .build();

        let image_info = vk::ImageCreateInfo::builder()
            .image_type(image_type)
            .format(vk::Format::from_raw(create_info.format as i32))
            .extent(extent)
            .mip_levels(create_info.mip_levels)
            .array_layers(create_info.array_layers)
            .samples(vk_sample_count(create_info.samples))
            .tiling(vk_image_tiling(create_info.tiling))
            .usage(vk_image_usage(create_info.usage))
            .flags(vk_image_create_flags(create_info.create_flags))
            .initial_layout(vk::ImageLayout::UNDEFINED);

        let alloc_options = vma_alloc_options(&alloc, false);
        let (image, allocation) = unsafe { self.vma().create_image(image_info, &alloc_options) }
            .map_err(vk_err)?;

        let handle = self.table.push(GpuImage { image, allocation })
            .map_err(|_| VulkanError::OutOfHostMemory)?;
        Ok(Resource::new_own(handle.rep()))
    }

    fn drop(&mut self, rep: Resource<Image>) -> wasmtime::anyhow::Result<()> {
        let key = Resource::<GpuImage>::new_own(rep.rep());
        let img = self.table.delete(key)?;
        unsafe { self.vma().destroy_image(img.image, img.allocation); }
        Ok(())
    }
}

// ── HostImageView ──

impl HostImageView for VkContextView<'_> {
    fn create(
        &mut self,
        create_info: ImageViewCreateInfo,
    ) -> Result<Resource<ImageView>, VulkanError> {
        let image_key = Resource::<GpuImage>::new_borrow(create_info.image.rep());
        let gpu_image = self.table.get(&image_key).map_err(|_| VulkanError::Unknown)?;

        let sr = &create_info.subresource_range;
        let subresource_range = vk::ImageSubresourceRange::builder()
            .aspect_mask(vk_image_aspect_flags(sr.aspect_mask))
            .base_mip_level(sr.base_mip_level)
            .level_count(sr.level_count)
            .base_array_layer(sr.base_array_layer)
            .layer_count(sr.layer_count)
            .build();

        let mut view_info = vk::ImageViewCreateInfo::builder()
            .image(gpu_image.image)
            .view_type(vk_image_view_type(create_info.view_type))
            .format(vk::Format::from_raw(create_info.format as i32))
            .subresource_range(subresource_range);

        if let Some(ref swizzle) = create_info.swizzle {
            let components = vk::ComponentMapping::builder()
                .r(vk::ComponentSwizzle::from_raw(swizzle.r as i32))
                .g(vk::ComponentSwizzle::from_raw(swizzle.g as i32))
                .b(vk::ComponentSwizzle::from_raw(swizzle.b as i32))
                .a(vk::ComponentSwizzle::from_raw(swizzle.a as i32))
                .build();
            view_info = view_info.components(components);
        }

        let view = unsafe { self.vk_device().create_image_view(&view_info, None) }.map_err(vk_err)?;
        let handle = self.table.push(GpuImageView { view })
            .map_err(|_| VulkanError::OutOfHostMemory)?;
        Ok(Resource::new_own(handle.rep()))
    }

    fn drop(&mut self, rep: Resource<ImageView>) -> wasmtime::anyhow::Result<()> {
        let key = Resource::<GpuImageView>::new_own(rep.rep());
        let view = self.table.delete(key)?;
        unsafe { self.vk_device().destroy_image_view(view.view, None); }
        Ok(())
    }
}

// ── HostSampler ──

impl HostSampler for VkContextView<'_> {
    fn create(
        &mut self,
        create_info: SamplerCreateInfo,
    ) -> Result<Resource<Sampler>, VulkanError> {
        let sampler_info = vk::SamplerCreateInfo::builder()
            .mag_filter(vk_filter(create_info.mag_filter))
            .min_filter(vk_filter(create_info.min_filter))
            .mipmap_mode(vk_sampler_mipmap_mode(create_info.mipmap_mode))
            .address_mode_u(vk_sampler_address_mode(create_info.address_mode_u))
            .address_mode_v(vk_sampler_address_mode(create_info.address_mode_v))
            .address_mode_w(vk_sampler_address_mode(create_info.address_mode_w))
            .mip_lod_bias(create_info.mip_lod_bias)
            .anisotropy_enable(create_info.enable_anisotropy)
            .max_anisotropy(create_info.max_anisotropy)
            .compare_enable(create_info.compare_enable)
            .compare_op(vk_compare_op(create_info.compare_op))
            .min_lod(create_info.min_lod)
            .max_lod(create_info.max_lod)
            .border_color(vk_border_color(create_info.border_color))
            .unnormalized_coordinates(create_info.unnormalized_coordinates);

        let sampler = unsafe { self.vk_device().create_sampler(&sampler_info, None) }.map_err(vk_err)?;
        let handle = self.table.push(GpuSampler { sampler })
            .map_err(|_| VulkanError::OutOfHostMemory)?;
        Ok(Resource::new_own(handle.rep()))
    }

    fn drop(&mut self, rep: Resource<Sampler>) -> wasmtime::anyhow::Result<()> {
        let key = Resource::<GpuSampler>::new_own(rep.rep());
        let sampler = self.table.delete(key)?;
        unsafe { self.vk_device().destroy_sampler(sampler.sampler, None); }
        Ok(())
    }
}

fn vk_image_aspect_flags(flags: ImageAspectFlags) -> vk::ImageAspectFlags {
    let mut vkf = vk::ImageAspectFlags::empty();
    if flags.contains(ImageAspectFlags::COLOR) { vkf |= vk::ImageAspectFlags::COLOR; }
    if flags.contains(ImageAspectFlags::DEPTH) { vkf |= vk::ImageAspectFlags::DEPTH; }
    if flags.contains(ImageAspectFlags::STENCIL) { vkf |= vk::ImageAspectFlags::STENCIL; }
    if flags.contains(ImageAspectFlags::METADATA) { vkf |= vk::ImageAspectFlags::METADATA; }
    if flags.contains(ImageAspectFlags::PLANE0) { vkf |= vk::ImageAspectFlags::PLANE_0; }
    if flags.contains(ImageAspectFlags::PLANE1) { vkf |= vk::ImageAspectFlags::PLANE_1; }
    if flags.contains(ImageAspectFlags::PLANE2) { vkf |= vk::ImageAspectFlags::PLANE_2; }
    vkf
}
