use vulkanalia::prelude::v1_0::*;
use vulkanalia::vk::{self, DeviceV1_3, HasBuilder};
use wasmtime::component::Resource;

use crate::{
    VkContextView,
    binding::ark::gpu::{
        buffer::Buffer as WitBuffer,
        command_buffer::{
            BufferCopy, BufferImageCopy, ClearColor, ClearDepthStencil, CommandBuffer,
            CommandBufferBuilder, CommandBufferUsage, Extent3d, Filter, Host, HostCommandBuffer,
            HostCommandBufferBuilder, ImageAspectFlags, ImageBlit, ImageCopy, ImageResolve,
            ImageSubresourceLayers, ImageSubresourceRange, MemoryBarrier, Offset3d, Rect2d,
            RenderingColorAttachment, RenderingDepthStencilAttachment, Viewport,
        },
        core::{QueueFamily, VulkanError},
        descriptor::DescriptorSet as WitDescriptorSet,
        image::{Image as WitImage, ImageView as WitImageView},
        pipeline::{
            ComputePipeline, GraphicsPipeline, PipelineBindPoint, PipelineLayout,
            RayTracingPipeline,
        },
    },
};

impl VkContextView<'_> {
    /// Get the actual Vulkan queue family index for the given logical family.
    #[inline]
    fn vk_queue_family_index(&self, qf: QueueFamily) -> u32 {
        match qf {
            QueueFamily::Graphics => self.owned.graphics_queue_family_index,
            QueueFamily::Compute => self.owned.compute_queue_family_index,
            QueueFamily::Transfer => self.owned.transfer_queue_family_index,
        }
    }

    /// Get or create the command pool for the given queue family.
    ///
    /// Uses `OnceLock::get_or_init` — thread-safe and guaranteed to
    /// initialise exactly once even under concurrent calls.
    fn get_or_create_cmd_pool(&self, qf: QueueFamily) -> vk::CommandPool {
        let lock = match qf {
            QueueFamily::Graphics => &self.graphics_command_pool,
            QueueFamily::Compute => &self.compute_command_pool,
            QueueFamily::Transfer => &self.transfer_command_pool,
        };

        *lock.get_or_init(|| {
            let idx = self.vk_queue_family_index(qf);
            let pool_info = vk::CommandPoolCreateInfo::builder()
                .queue_family_index(idx)
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                .build();
            unsafe { self.vk_device().create_command_pool(&pool_info, None) }
                .expect("failed to create command pool")
        })
    }
}

pub(crate) struct GpuCommandBufferBuilder {
    pub(crate) cmd: vk::CommandBuffer,
    pub(crate) pool: vk::CommandPool,
}

pub(crate) struct GpuCommandBuffer {
    pub(crate) cmd: vk::CommandBuffer,
    pub(crate) pool: vk::CommandPool,
}

impl Host for VkContextView<'_> {}

// ── Helpers ──

fn vk_subresource_layers(sl: &ImageSubresourceLayers) -> vk::ImageSubresourceLayers {
    vk::ImageSubresourceLayers::builder()
        .aspect_mask(vk_image_aspect_flags(sl.aspect_mask))
        .mip_level(sl.mip_level)
        .base_array_layer(sl.base_array_layer)
        .layer_count(sl.layer_count)
        .build()
}

fn vk_subresource_range(sr: &ImageSubresourceRange) -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::builder()
        .aspect_mask(vk_image_aspect_flags(sr.aspect_mask))
        .base_mip_level(sr.base_mip_level)
        .level_count(sr.level_count)
        .base_array_layer(sr.base_array_layer)
        .layer_count(sr.layer_count)
        .build()
}

fn vk_offset3d(o: &Offset3d) -> vk::Offset3D {
    vk::Offset3D::builder()
        .x(o.x as i32)
        .y(o.y as i32)
        .z(o.z as i32)
        .build()
}

fn vk_extent3d(e: &Extent3d) -> vk::Extent3D {
    vk::Extent3D::builder()
        .width(e.width)
        .height(e.height)
        .depth(e.depth)
        .build()
}

fn vk_rect2d(r: &Rect2d) -> vk::Rect2D {
    vk::Rect2D::builder()
        .offset(vk::Offset2D {
            x: r.offset_x,
            y: r.offset_y,
        })
        .extent(vk::Extent2D {
            width: r.width,
            height: r.height,
        })
        .build()
}

fn vk_clear_color(c: &ClearColor) -> vk::ClearColorValue {
    let (r, g, b, a) = c.floats;
    vk::ClearColorValue {
        float32: [r, g, b, a],
    }
}

fn vk_clear_depth_stencil(c: &ClearDepthStencil) -> vk::ClearDepthStencilValue {
    vk::ClearDepthStencilValue {
        depth: c.depth,
        stencil: c.stencil,
    }
}

fn vk_viewport(v: &Viewport) -> vk::Viewport {
    vk::Viewport::builder()
        .x(v.x)
        .y(v.y)
        .width(v.width)
        .height(v.height)
        .min_depth(v.min_depth)
        .max_depth(v.max_depth)
        .build()
}

fn vk_image_aspect_flags(flags: ImageAspectFlags) -> vk::ImageAspectFlags {
    let mut vk_flags = vk::ImageAspectFlags::empty();
    if flags.contains(ImageAspectFlags::COLOR) {
        vk_flags |= vk::ImageAspectFlags::COLOR;
    }
    if flags.contains(ImageAspectFlags::DEPTH) {
        vk_flags |= vk::ImageAspectFlags::DEPTH;
    }
    if flags.contains(ImageAspectFlags::STENCIL) {
        vk_flags |= vk::ImageAspectFlags::STENCIL;
    }
    if flags.contains(ImageAspectFlags::METADATA) {
        vk_flags |= vk::ImageAspectFlags::METADATA;
    }
    if flags.contains(ImageAspectFlags::PLANE0) {
        vk_flags |= vk::ImageAspectFlags::PLANE_0;
    }
    if flags.contains(ImageAspectFlags::PLANE1) {
        vk_flags |= vk::ImageAspectFlags::PLANE_1;
    }
    if flags.contains(ImageAspectFlags::PLANE2) {
        vk_flags |= vk::ImageAspectFlags::PLANE_2;
    }
    vk_flags
}

// ── Host implementations ──

impl HostCommandBufferBuilder for VkContextView<'_> {
    fn new(
        &mut self,
        queue_family: QueueFamily,
        usage: CommandBufferUsage,
    ) -> Resource<CommandBufferBuilder> {
        let pool = self.get_or_create_cmd_pool(queue_family);

        let alloc_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);

        let cmd = unsafe { self.vk_device().allocate_command_buffers(&alloc_info) }
            .expect("failed to allocate command buffer")[0];

        let flags = match usage {
            CommandBufferUsage::OneTimeSubmit => vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT,
            CommandBufferUsage::MultipleSubmit => vk::CommandBufferUsageFlags::empty(),
            CommandBufferUsage::SimultaneousUse => vk::CommandBufferUsageFlags::SIMULTANEOUS_USE,
        };

        let begin_info = vk::CommandBufferBeginInfo::builder().flags(flags);
        if let Err(e) = unsafe { self.vk_device().begin_command_buffer(cmd, &begin_info) } {
            unsafe {
                self.vk_device().free_command_buffers(pool, &[cmd]);
            }
            panic!("failed to begin command buffer: {e:?}");
        }

        let builder = GpuCommandBufferBuilder { cmd, pool };
        let handle = match self.table.push(builder) {
            Ok(h) => h,
            Err(e) => {
                unsafe {
                    self.vk_device().free_command_buffers(pool, &[cmd]);
                }
                panic!("ResourceTable push failed: {e:?}");
            }
        };
        Resource::new_own(handle.rep())
    }

    // ── Pipeline binding ──
    // ── Pipeline binding ──

    fn bind_compute_pipeline(
        &mut self,
        self_: Resource<CommandBufferBuilder>,
        pipeline: Resource<ComputePipeline>,
    ) -> Result<(), VulkanError> {
        let key = Resource::<GpuCommandBufferBuilder>::new_borrow(self_.rep());
        let builder = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        let pipeline_key =
            Resource::<super::pipeline::GpuComputePipeline>::new_borrow(pipeline.rep());
        let gpu_pipeline = self
            .table
            .get(&pipeline_key)
            .map_err(|_| VulkanError::Unknown)?;
        unsafe {
            self.vk_device().cmd_bind_pipeline(
                builder.cmd,
                vk::PipelineBindPoint::COMPUTE,
                gpu_pipeline.pipeline,
            );
        }
        Ok(())
    }

    fn bind_graphics_pipeline(
        &mut self,
        self_: Resource<CommandBufferBuilder>,
        pipeline: Resource<GraphicsPipeline>,
    ) -> Result<(), VulkanError> {
        let key = Resource::<GpuCommandBufferBuilder>::new_borrow(self_.rep());
        let builder = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        let pipeline_key =
            Resource::<super::pipeline::GpuGraphicsPipeline>::new_borrow(pipeline.rep());
        let gpu_pipeline = self
            .table
            .get(&pipeline_key)
            .map_err(|_| VulkanError::Unknown)?;
        unsafe {
            self.vk_device().cmd_bind_pipeline(
                builder.cmd,
                vk::PipelineBindPoint::GRAPHICS,
                gpu_pipeline.pipeline,
            );
        }
        Ok(())
    }

    fn bind_ray_tracing_pipeline(
        &mut self,
        self_: Resource<CommandBufferBuilder>,
        pipeline: Resource<RayTracingPipeline>,
    ) -> Result<(), VulkanError> {
        let key = Resource::<GpuCommandBufferBuilder>::new_borrow(self_.rep());
        let builder = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        let pipeline_key =
            Resource::<super::pipeline::GpuRayTracingPipeline>::new_borrow(pipeline.rep());
        let gpu_pipeline = self
            .table
            .get(&pipeline_key)
            .map_err(|_| VulkanError::Unknown)?;
        unsafe {
            self.vk_device().cmd_bind_pipeline(
                builder.cmd,
                vk::PipelineBindPoint::RAY_TRACING_KHR,
                gpu_pipeline.pipeline,
            );
        }
        Ok(())
    }

    // ── Descriptor set binding ──

    fn bind_descriptor_sets(
        &mut self,
        self_: Resource<CommandBufferBuilder>,
        bind_point: PipelineBindPoint,
        layout: Resource<PipelineLayout>,
        first_set: u32,
        sets: Vec<Resource<WitDescriptorSet>>,
    ) -> Result<(), VulkanError> {
        let key = Resource::<GpuCommandBufferBuilder>::new_borrow(self_.rep());
        let builder = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        let layout_key = Resource::<super::pipeline::GpuPipelineLayout>::new_borrow(layout.rep());
        let gpu_layout = self
            .table
            .get(&layout_key)
            .map_err(|_| VulkanError::Unknown)?;

        let vk_bind_point = match bind_point {
            PipelineBindPoint::Graphics => vk::PipelineBindPoint::GRAPHICS,
            PipelineBindPoint::Compute => vk::PipelineBindPoint::COMPUTE,
            PipelineBindPoint::RayTracing => vk::PipelineBindPoint::RAY_TRACING_KHR,
        };

        // Collect vk::DescriptorSet handles and keep borrows alive.
        let mut vk_sets: Vec<vk::DescriptorSet> = Vec::with_capacity(sets.len());
        let mut set_keys: Vec<Resource<WitDescriptorSet>> = Vec::with_capacity(sets.len());
        for set in &sets {
            let set_key = Resource::<super::descriptor::GpuDescriptorSet>::new_borrow(set.rep());
            let gpu_set = self.table.get(&set_key).map_err(|_| VulkanError::Unknown)?;
            vk_sets.push(gpu_set.set);
            set_keys.push(Resource::<WitDescriptorSet>::new_borrow(set.rep()));
        }

        // No dynamic offsets for now.
        let dynamic_offsets: [u32; 0] = [];

        unsafe {
            self.vk_device().cmd_bind_descriptor_sets(
                builder.cmd,
                vk_bind_point,
                gpu_layout.layout,
                first_set,
                &vk_sets,
                &dynamic_offsets,
            );
        }

        let _ = set_keys; // keep borrows alive

        Ok(())
    }

    // ── Push constants ──

    fn push_constants(
        &mut self,
        self_: Resource<CommandBufferBuilder>,
        layout: Resource<PipelineLayout>,
        stage_flags: u32,
        offset: u32,
        data: Vec<u8>,
    ) -> Result<(), VulkanError> {
        let key = Resource::<GpuCommandBufferBuilder>::new_borrow(self_.rep());
        let builder = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        let layout_key = Resource::<super::pipeline::GpuPipelineLayout>::new_borrow(layout.rep());
        let gpu_layout = self
            .table
            .get(&layout_key)
            .map_err(|_| VulkanError::Unknown)?;
        unsafe {
            self.vk_device().cmd_push_constants(
                builder.cmd,
                gpu_layout.layout,
                vk::ShaderStageFlags::from_bits_truncate(stage_flags),
                offset,
                &data,
            );
        }
        Ok(())
    }

    // ── Dispatch ──

    fn dispatch(
        &mut self,
        self_: Resource<CommandBufferBuilder>,
        x: u32,
        y: u32,
        z: u32,
    ) -> Result<(), VulkanError> {
        let key = Resource::<GpuCommandBufferBuilder>::new_borrow(self_.rep());
        let builder = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        unsafe { self.vk_device().cmd_dispatch(builder.cmd, x, y, z) };
        Ok(())
    }

    fn dispatch_indirect(
        &mut self,
        self_: Resource<CommandBufferBuilder>,
        buffer: Resource<WitBuffer>,
        offset: u64,
    ) -> Result<(), VulkanError> {
        let key = Resource::<GpuCommandBufferBuilder>::new_borrow(self_.rep());
        let builder = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        let buf_key = Resource::<super::buffer::GpuBuffer>::new_borrow(buffer.rep());
        let gpu_buf = self.table.get(&buf_key).map_err(|_| VulkanError::Unknown)?;
        unsafe {
            self.vk_device()
                .cmd_dispatch_indirect(builder.cmd, gpu_buf.buffer, offset)
        };
        Ok(())
    }

    // ── Draw ──

    fn draw(
        &mut self,
        self_: Resource<CommandBufferBuilder>,
        vertex_count: u32,
        instance_count: u32,
        first_vertex: u32,
        first_instance: u32,
    ) -> Result<(), VulkanError> {
        let key = Resource::<GpuCommandBufferBuilder>::new_borrow(self_.rep());
        let builder = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        unsafe {
            self.vk_device().cmd_draw(
                builder.cmd,
                vertex_count,
                instance_count,
                first_vertex,
                first_instance,
            );
        }
        Ok(())
    }

    fn draw_indexed(
        &mut self,
        self_: Resource<CommandBufferBuilder>,
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    ) -> Result<(), VulkanError> {
        let key = Resource::<GpuCommandBufferBuilder>::new_borrow(self_.rep());
        let builder = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        unsafe {
            self.vk_device().cmd_draw_indexed(
                builder.cmd,
                index_count,
                instance_count,
                first_index,
                vertex_offset,
                first_instance,
            );
        }
        Ok(())
    }

    fn draw_indirect(
        &mut self,
        self_: Resource<CommandBufferBuilder>,
        buffer: Resource<WitBuffer>,
        offset: u64,
        draw_count: u32,
        stride: u32,
    ) -> Result<(), VulkanError> {
        let key = Resource::<GpuCommandBufferBuilder>::new_borrow(self_.rep());
        let builder = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        let buf_key = Resource::<super::buffer::GpuBuffer>::new_borrow(buffer.rep());
        let gpu_buf = self.table.get(&buf_key).map_err(|_| VulkanError::Unknown)?;
        unsafe {
            self.vk_device().cmd_draw_indirect(
                builder.cmd,
                gpu_buf.buffer,
                offset,
                draw_count,
                stride,
            );
        }
        Ok(())
    }

    fn draw_indexed_indirect(
        &mut self,
        self_: Resource<CommandBufferBuilder>,
        buffer: Resource<WitBuffer>,
        offset: u64,
        draw_count: u32,
        stride: u32,
    ) -> Result<(), VulkanError> {
        let key = Resource::<GpuCommandBufferBuilder>::new_borrow(self_.rep());
        let builder = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        let buf_key = Resource::<super::buffer::GpuBuffer>::new_borrow(buffer.rep());
        let gpu_buf = self.table.get(&buf_key).map_err(|_| VulkanError::Unknown)?;
        unsafe {
            self.vk_device().cmd_draw_indexed_indirect(
                builder.cmd,
                gpu_buf.buffer,
                offset,
                draw_count,
                stride,
            );
        }
        Ok(())
    }

    // ── Copy ──

    fn copy_buffer(
        &mut self,
        self_: Resource<CommandBufferBuilder>,
        src: Resource<WitBuffer>,
        dst: Resource<WitBuffer>,
        regions: Vec<BufferCopy>,
    ) -> Result<(), VulkanError> {
        let key = Resource::<GpuCommandBufferBuilder>::new_borrow(self_.rep());
        let builder = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        let src_key = Resource::<super::buffer::GpuBuffer>::new_borrow(src.rep());
        let src_buf = self.table.get(&src_key).map_err(|_| VulkanError::Unknown)?;
        let dst_key = Resource::<super::buffer::GpuBuffer>::new_borrow(dst.rep());
        let dst_buf = self.table.get(&dst_key).map_err(|_| VulkanError::Unknown)?;

        let copies: Vec<vk::BufferCopy> = regions
            .iter()
            .map(|r| {
                vk::BufferCopy::builder()
                    .src_offset(r.src_offset)
                    .dst_offset(r.dst_offset)
                    .size(r.size)
                    .build()
            })
            .collect();

        unsafe {
            self.vk_device()
                .cmd_copy_buffer(builder.cmd, src_buf.buffer, dst_buf.buffer, &copies);
        }
        Ok(())
    }

    fn copy_buffer_to_image(
        &mut self,
        self_: Resource<CommandBufferBuilder>,
        src: Resource<WitBuffer>,
        dst: Resource<WitImage>,
        dst_layout: u32,
        regions: Vec<BufferImageCopy>,
    ) -> Result<(), VulkanError> {
        let key = Resource::<GpuCommandBufferBuilder>::new_borrow(self_.rep());
        let builder = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        let src_key = Resource::<super::buffer::GpuBuffer>::new_borrow(src.rep());
        let src_buf = self.table.get(&src_key).map_err(|_| VulkanError::Unknown)?;
        let dst_key = Resource::<super::image::GpuImage>::new_borrow(dst.rep());
        let dst_img = self.table.get(&dst_key).map_err(|_| VulkanError::Unknown)?;

        let copies: Vec<vk::BufferImageCopy> = regions
            .iter()
            .map(|r| {
                vk::BufferImageCopy::builder()
                    .buffer_offset(r.buffer_offset)
                    .buffer_row_length(r.buffer_row_length)
                    .buffer_image_height(r.buffer_image_height)
                    .image_subresource(vk_subresource_layers(&r.image_subresource))
                    .image_offset(vk_offset3d(&r.image_offset))
                    .image_extent(vk_extent3d(&r.image_extent))
                    .build()
            })
            .collect();

        unsafe {
            self.vk_device().cmd_copy_buffer_to_image(
                builder.cmd,
                src_buf.buffer,
                dst_img.image,
                vk::ImageLayout::from_raw(dst_layout as i32),
                &copies,
            );
        }
        Ok(())
    }

    fn copy_image_to_buffer(
        &mut self,
        self_: Resource<CommandBufferBuilder>,
        src: Resource<WitImage>,
        src_layout: u32,
        dst: Resource<WitBuffer>,
        regions: Vec<BufferImageCopy>,
    ) -> Result<(), VulkanError> {
        let key = Resource::<GpuCommandBufferBuilder>::new_borrow(self_.rep());
        let builder = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        let src_key = Resource::<super::image::GpuImage>::new_borrow(src.rep());
        let src_img = self.table.get(&src_key).map_err(|_| VulkanError::Unknown)?;
        let dst_key = Resource::<super::buffer::GpuBuffer>::new_borrow(dst.rep());
        let dst_buf = self.table.get(&dst_key).map_err(|_| VulkanError::Unknown)?;

        let copies: Vec<vk::BufferImageCopy> = regions
            .iter()
            .map(|r| {
                vk::BufferImageCopy::builder()
                    .buffer_offset(r.buffer_offset)
                    .buffer_row_length(r.buffer_row_length)
                    .buffer_image_height(r.buffer_image_height)
                    .image_subresource(vk_subresource_layers(&r.image_subresource))
                    .image_offset(vk_offset3d(&r.image_offset))
                    .image_extent(vk_extent3d(&r.image_extent))
                    .build()
            })
            .collect();

        unsafe {
            self.vk_device().cmd_copy_image_to_buffer(
                builder.cmd,
                src_img.image,
                vk::ImageLayout::from_raw(src_layout as i32),
                dst_buf.buffer,
                &copies,
            );
        }
        Ok(())
    }

    fn copy_image(
        &mut self,
        self_: Resource<CommandBufferBuilder>,
        src: Resource<WitImage>,
        src_layout: u32,
        dst: Resource<WitImage>,
        dst_layout: u32,
        regions: Vec<ImageCopy>,
    ) -> Result<(), VulkanError> {
        let key = Resource::<GpuCommandBufferBuilder>::new_borrow(self_.rep());
        let builder = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        let src_key = Resource::<super::image::GpuImage>::new_borrow(src.rep());
        let src_img = self.table.get(&src_key).map_err(|_| VulkanError::Unknown)?;
        let dst_key = Resource::<super::image::GpuImage>::new_borrow(dst.rep());
        let dst_img = self.table.get(&dst_key).map_err(|_| VulkanError::Unknown)?;

        let copies: Vec<vk::ImageCopy> = regions
            .iter()
            .map(|r| {
                vk::ImageCopy::builder()
                    .src_subresource(vk_subresource_layers(&r.src_subresource))
                    .src_offset(vk_offset3d(&r.src_offset))
                    .dst_subresource(vk_subresource_layers(&r.dst_subresource))
                    .dst_offset(vk_offset3d(&r.dst_offset))
                    .extent(vk_extent3d(&r.extent))
                    .build()
            })
            .collect();

        unsafe {
            self.vk_device().cmd_copy_image(
                builder.cmd,
                src_img.image,
                vk::ImageLayout::from_raw(src_layout as i32),
                dst_img.image,
                vk::ImageLayout::from_raw(dst_layout as i32),
                &copies,
            );
        }
        Ok(())
    }

    // ── Fill / Clear ──

    fn fill_buffer(
        &mut self,
        self_: Resource<CommandBufferBuilder>,
        buffer: Resource<WitBuffer>,
        offset: u64,
        size: u64,
        data: u32,
    ) -> Result<(), VulkanError> {
        let key = Resource::<GpuCommandBufferBuilder>::new_borrow(self_.rep());
        let builder = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        let buf_key = Resource::<super::buffer::GpuBuffer>::new_borrow(buffer.rep());
        let gpu_buf = self.table.get(&buf_key).map_err(|_| VulkanError::Unknown)?;
        unsafe {
            self.vk_device()
                .cmd_fill_buffer(builder.cmd, gpu_buf.buffer, offset, size, data);
        }
        Ok(())
    }

    fn clear_color_image(
        &mut self,
        self_: Resource<CommandBufferBuilder>,
        image: Resource<WitImage>,
        layout: u32,
        color: ClearColor,
        ranges: Vec<ImageSubresourceRange>,
    ) -> Result<(), VulkanError> {
        let key = Resource::<GpuCommandBufferBuilder>::new_borrow(self_.rep());
        let builder = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        let img_key = Resource::<super::image::GpuImage>::new_borrow(image.rep());
        let gpu_img = self.table.get(&img_key).map_err(|_| VulkanError::Unknown)?;

        let clear_value = vk_clear_color(&color);
        let vk_ranges: Vec<vk::ImageSubresourceRange> =
            ranges.iter().map(vk_subresource_range).collect();

        unsafe {
            self.vk_device().cmd_clear_color_image(
                builder.cmd,
                gpu_img.image,
                vk::ImageLayout::from_raw(layout as i32),
                &clear_value,
                &vk_ranges,
            );
        }
        Ok(())
    }

    fn clear_depth_stencil_image(
        &mut self,
        self_: Resource<CommandBufferBuilder>,
        image: Resource<WitImage>,
        layout: u32,
        value: ClearDepthStencil,
        ranges: Vec<ImageSubresourceRange>,
    ) -> Result<(), VulkanError> {
        let key = Resource::<GpuCommandBufferBuilder>::new_borrow(self_.rep());
        let builder = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        let img_key = Resource::<super::image::GpuImage>::new_borrow(image.rep());
        let gpu_img = self.table.get(&img_key).map_err(|_| VulkanError::Unknown)?;

        let clear_value = vk_clear_depth_stencil(&value);
        let vk_ranges: Vec<vk::ImageSubresourceRange> =
            ranges.iter().map(vk_subresource_range).collect();

        unsafe {
            self.vk_device().cmd_clear_depth_stencil_image(
                builder.cmd,
                gpu_img.image,
                vk::ImageLayout::from_raw(layout as i32),
                &clear_value,
                &vk_ranges,
            );
        }
        Ok(())
    }

    // ── Blit / Resolve ──

    fn blit_image(
        &mut self,
        self_: Resource<CommandBufferBuilder>,
        src: Resource<WitImage>,
        src_layout: u32,
        dst: Resource<WitImage>,
        dst_layout: u32,
        regions: Vec<ImageBlit>,
        filter: Filter,
    ) -> Result<(), VulkanError> {
        let key = Resource::<GpuCommandBufferBuilder>::new_borrow(self_.rep());
        let builder = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        let src_key = Resource::<super::image::GpuImage>::new_borrow(src.rep());
        let src_img = self.table.get(&src_key).map_err(|_| VulkanError::Unknown)?;
        let dst_key = Resource::<super::image::GpuImage>::new_borrow(dst.rep());
        let dst_img = self.table.get(&dst_key).map_err(|_| VulkanError::Unknown)?;

        let vk_filter = match filter {
            Filter::Nearest => vk::Filter::NEAREST,
            Filter::Linear => vk::Filter::LINEAR,
        };

        let blits: Vec<vk::ImageBlit> = regions
            .iter()
            .map(|r| {
                vk::ImageBlit::builder()
                    .src_subresource(vk_subresource_layers(&r.src_subresource))
                    .src_offsets([vk_offset3d(&r.src_offset0), vk_offset3d(&r.src_offset1)])
                    .dst_subresource(vk_subresource_layers(&r.dst_subresource))
                    .dst_offsets([vk_offset3d(&r.dst_offset0), vk_offset3d(&r.dst_offset1)])
                    .build()
            })
            .collect();

        unsafe {
            self.vk_device().cmd_blit_image(
                builder.cmd,
                src_img.image,
                vk::ImageLayout::from_raw(src_layout as i32),
                dst_img.image,
                vk::ImageLayout::from_raw(dst_layout as i32),
                &blits,
                vk_filter,
            );
        }
        Ok(())
    }

    fn resolve_image(
        &mut self,
        self_: Resource<CommandBufferBuilder>,
        src: Resource<WitImage>,
        src_layout: u32,
        dst: Resource<WitImage>,
        dst_layout: u32,
        regions: Vec<ImageResolve>,
    ) -> Result<(), VulkanError> {
        let key = Resource::<GpuCommandBufferBuilder>::new_borrow(self_.rep());
        let builder = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        let src_key = Resource::<super::image::GpuImage>::new_borrow(src.rep());
        let src_img = self.table.get(&src_key).map_err(|_| VulkanError::Unknown)?;
        let dst_key = Resource::<super::image::GpuImage>::new_borrow(dst.rep());
        let dst_img = self.table.get(&dst_key).map_err(|_| VulkanError::Unknown)?;

        let resolves: Vec<vk::ImageResolve> = regions
            .iter()
            .map(|r| {
                vk::ImageResolve::builder()
                    .src_subresource(vk_subresource_layers(&r.src_subresource))
                    .src_offset(vk_offset3d(&r.src_offset))
                    .dst_subresource(vk_subresource_layers(&r.dst_subresource))
                    .dst_offset(vk_offset3d(&r.dst_offset))
                    .extent(vk_extent3d(&r.extent))
                    .build()
            })
            .collect();

        unsafe {
            self.vk_device().cmd_resolve_image(
                builder.cmd,
                src_img.image,
                vk::ImageLayout::from_raw(src_layout as i32),
                dst_img.image,
                vk::ImageLayout::from_raw(dst_layout as i32),
                &resolves,
            );
        }
        Ok(())
    }

    // ── Synchronization ──

    fn pipeline_barrier(
        &mut self,
        self_: Resource<CommandBufferBuilder>,
        barriers: Vec<MemoryBarrier>,
    ) -> Result<(), VulkanError> {
        let key = Resource::<GpuCommandBufferBuilder>::new_borrow(self_.rep());
        let builder = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;

        let mut buffer_barriers: Vec<vk::BufferMemoryBarrier> = Vec::new();
        let mut image_barriers: Vec<vk::ImageMemoryBarrier> = Vec::new();
        let mut global_barriers: Vec<vk::MemoryBarrier> = Vec::new();

        for barrier in &barriers {
            if let Some(ref bb) = barrier.buffer {
                let buf_key = Resource::<super::buffer::GpuBuffer>::new_borrow(bb.buffer.rep());
                let gpu_buf = self.table.get(&buf_key).map_err(|_| VulkanError::Unknown)?;
                buffer_barriers.push(
                    vk::BufferMemoryBarrier::builder()
                        .src_access_mask(vk::AccessFlags::from_bits_truncate(bb.src_access))
                        .dst_access_mask(vk::AccessFlags::from_bits_truncate(bb.dst_access))
                        .buffer(gpu_buf.buffer)
                        .offset(bb.offset)
                        .size(bb.size)
                        .build(),
                );
            }
            if let Some(ref ib) = barrier.image {
                let img_key = Resource::<super::image::GpuImage>::new_borrow(ib.image.rep());
                let gpu_img = self.table.get(&img_key).map_err(|_| VulkanError::Unknown)?;
                image_barriers.push(
                    vk::ImageMemoryBarrier::builder()
                        .src_access_mask(vk::AccessFlags::from_bits_truncate(ib.src_access))
                        .dst_access_mask(vk::AccessFlags::from_bits_truncate(ib.dst_access))
                        .old_layout(vk::ImageLayout::from_raw(ib.old_layout as i32))
                        .new_layout(vk::ImageLayout::from_raw(ib.new_layout as i32))
                        .image(gpu_img.image)
                        .subresource_range(vk_subresource_range(&ib.subresource_range))
                        .build(),
                );
            }
            // Global barriers apply to all memory — insert a VkMemoryBarrier
            // with fully-inclusive access masks.
            if barrier.global && barrier.buffer.is_none() && barrier.image.is_none() {
                global_barriers.push(
                    vk::MemoryBarrier::builder()
                        .src_access_mask(
                            vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE,
                        )
                        .dst_access_mask(
                            vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE,
                        )
                        .build(),
                );
            }
        }

        let mut src_stage = vk::PipelineStageFlags::empty();
        let mut dst_stage = vk::PipelineStageFlags::empty();

        for barrier in &barriers {
            src_stage |= vk::PipelineStageFlags::from_bits_truncate(barrier.src_stage);
            dst_stage |= vk::PipelineStageFlags::from_bits_truncate(barrier.dst_stage);
        }

        if !buffer_barriers.is_empty() || !image_barriers.is_empty() || !global_barriers.is_empty()
        {
            unsafe {
                self.vk_device().cmd_pipeline_barrier(
                    builder.cmd,
                    src_stage,
                    dst_stage,
                    vk::DependencyFlags::empty(),
                    &global_barriers,
                    &buffer_barriers,
                    &image_barriers,
                );
            }
        }
        Ok(())
    }

    // ── Dynamic rendering ──

    fn begin_rendering(
        &mut self,
        self_: Resource<CommandBufferBuilder>,
        render_area: Rect2d,
        layer_count: u32,
        color_attachments: Vec<RenderingColorAttachment>,
        depth_stencil_attachment: Option<RenderingDepthStencilAttachment>,
    ) -> Result<(), VulkanError> {
        let key = Resource::<GpuCommandBufferBuilder>::new_borrow(self_.rep());
        let builder = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;

        let mut color_attachments_vk: Vec<vk::RenderingAttachmentInfo> = Vec::new();
        let mut stored_image_views: Vec<Resource<WitImageView>> = Vec::new();

        for ca in &color_attachments {
            let view_key = Resource::<super::image::GpuImageView>::new_borrow(ca.image_view.rep());
            let view = self
                .table
                .get(&view_key)
                .map_err(|_| VulkanError::Unknown)?;

            let mut attachment = vk::RenderingAttachmentInfo::builder()
                .image_view(view.view)
                .image_layout(vk::ImageLayout::from_raw(ca.image_layout as i32))
                .load_op(vk::AttachmentLoadOp::from_raw(ca.load_op as i32))
                .store_op(vk::AttachmentStoreOp::from_raw(ca.store_op as i32));

            if let Some(ref resolve_view) = ca.resolve_image_view {
                let resolve_key =
                    Resource::<super::image::GpuImageView>::new_borrow(resolve_view.rep());
                let resolve = self
                    .table
                    .get(&resolve_key)
                    .map_err(|_| VulkanError::Unknown)?;
                attachment = attachment
                    .resolve_image_view(resolve.view)
                    .resolve_image_layout(vk::ImageLayout::from_raw(
                        ca.resolve_image_layout as i32,
                    ));
                // Keep resolve borrow alive
                stored_image_views.push(Resource::<WitImageView>::new_borrow(resolve_view.rep()));
            }

            if let Some(ref clear) = ca.clear_value {
                attachment = attachment.clear_value(vk::ClearValue {
                    color: vk_clear_color(clear),
                });
            }

            color_attachments_vk.push(attachment.build());
            stored_image_views.push(Resource::<WitImageView>::new_borrow(ca.image_view.rep()));
        }

        let mut depth_vk: Option<vk::RenderingAttachmentInfo> = None;
        let mut stencil_vk: Option<vk::RenderingAttachmentInfo> = None;
        let mut stored_depth_view: Option<Resource<WitImageView>> = None;

        if let Some(ref dsa) = depth_stencil_attachment {
            let view_key = Resource::<super::image::GpuImageView>::new_borrow(dsa.image_view.rep());
            let view = self
                .table
                .get(&view_key)
                .map_err(|_| VulkanError::Unknown)?;

            let mut depth_attachment = vk::RenderingAttachmentInfo::builder()
                .image_view(view.view)
                .image_layout(vk::ImageLayout::from_raw(dsa.image_layout as i32))
                .load_op(vk::AttachmentLoadOp::from_raw(dsa.depth_load_op as i32))
                .store_op(vk::AttachmentStoreOp::from_raw(dsa.depth_store_op as i32));

            if let Some(clear_depth) = dsa.clear_depth {
                let clear_value = vk::ClearValue {
                    depth_stencil: vk::ClearDepthStencilValue {
                        depth: clear_depth,
                        stencil: dsa.clear_stencil.unwrap_or(0),
                    },
                };
                depth_attachment = depth_attachment.clear_value(clear_value);
            }

            let stencil_attachment = vk::RenderingAttachmentInfo::builder()
                .image_view(view.view)
                .image_layout(vk::ImageLayout::from_raw(dsa.image_layout as i32))
                .load_op(vk::AttachmentLoadOp::from_raw(dsa.stencil_load_op as i32))
                .store_op(vk::AttachmentStoreOp::from_raw(dsa.stencil_store_op as i32));

            depth_vk = Some(depth_attachment.build());
            stencil_vk = Some(stencil_attachment.build());
            stored_depth_view = Some(Resource::<WitImageView>::new_borrow(dsa.image_view.rep()));
        }

        let mut rendering_info = vk::RenderingInfo::builder()
            .render_area(vk_rect2d(&render_area))
            .layer_count(layer_count)
            .color_attachments(&color_attachments_vk);

        if let Some(ref depth) = depth_vk {
            rendering_info = rendering_info.depth_attachment(depth);
        }
        if let Some(ref stencil) = stencil_vk {
            rendering_info = rendering_info.stencil_attachment(stencil);
        }

        let rendering_info = rendering_info;

        unsafe {
            self.vk_device()
                .cmd_begin_rendering(builder.cmd, &rendering_info);
        }

        // Keep borrow refs alive
        let _ = stored_image_views;
        let _ = stored_depth_view;

        Ok(())
    }

    fn end_rendering(&mut self, self_: Resource<CommandBufferBuilder>) -> Result<(), VulkanError> {
        let key = Resource::<GpuCommandBufferBuilder>::new_borrow(self_.rep());
        let builder = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        unsafe {
            self.vk_device().cmd_end_rendering(builder.cmd);
        }
        Ok(())
    }

    // ── Dynamic state ──

    fn set_viewport(
        &mut self,
        self_: Resource<CommandBufferBuilder>,
        first: u32,
        viewports: Vec<Viewport>,
    ) -> Result<(), VulkanError> {
        let key = Resource::<GpuCommandBufferBuilder>::new_borrow(self_.rep());
        let builder = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        let vk_viewports: Vec<vk::Viewport> = viewports.iter().map(vk_viewport).collect();
        unsafe {
            self.vk_device()
                .cmd_set_viewport(builder.cmd, first, &vk_viewports);
        }
        Ok(())
    }

    fn set_scissor(
        &mut self,
        self_: Resource<CommandBufferBuilder>,
        first: u32,
        scissors: Vec<Rect2d>,
    ) -> Result<(), VulkanError> {
        let key = Resource::<GpuCommandBufferBuilder>::new_borrow(self_.rep());
        let builder = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        let vk_scissors: Vec<vk::Rect2D> = scissors.iter().map(vk_rect2d).collect();
        unsafe {
            self.vk_device()
                .cmd_set_scissor(builder.cmd, first, &vk_scissors);
        }
        Ok(())
    }

    fn set_line_width(
        &mut self,
        self_: Resource<CommandBufferBuilder>,
        width: f32,
    ) -> Result<(), VulkanError> {
        let key = Resource::<GpuCommandBufferBuilder>::new_borrow(self_.rep());
        let builder = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        unsafe {
            self.vk_device().cmd_set_line_width(builder.cmd, width);
        }
        Ok(())
    }

    fn set_depth_bias(
        &mut self,
        self_: Resource<CommandBufferBuilder>,
        constant: f32,
        slope: f32,
        clamp: f32,
    ) -> Result<(), VulkanError> {
        let key = Resource::<GpuCommandBufferBuilder>::new_borrow(self_.rep());
        let builder = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        unsafe {
            self.vk_device()
                .cmd_set_depth_bias(builder.cmd, constant, clamp, slope);
        }
        Ok(())
    }

    fn drop(&mut self, rep: Resource<CommandBufferBuilder>) -> wasmtime::anyhow::Result<()> {
        let key = Resource::<GpuCommandBufferBuilder>::new_own(rep.rep());
        let builder = self.table.delete(key)?;
        unsafe {
            // If the command buffer was never ended, just free it
            self.vk_device()
                .free_command_buffers(builder.pool, &[builder.cmd]);
        }
        Ok(())
    }
}

impl HostCommandBuffer for VkContextView<'_> {
    fn build(
        &mut self,
        cmd: Resource<CommandBufferBuilder>,
    ) -> Resource<CommandBuffer> {
        let key = Resource::<GpuCommandBufferBuilder>::new_own(cmd.rep());
        let builder = self
            .table
            .delete(key)
            .expect("failed to delete builder from table");

        unsafe { self.vk_device().end_command_buffer(builder.cmd) }
            .expect("failed to end command buffer");

        let finalized = GpuCommandBuffer {
            cmd: builder.cmd,
            pool: builder.pool,
        };
        let handle = self
            .table
            .push(finalized)
            .expect("ResourceTable push failed");
        Resource::new_own(handle.rep())
    }

    fn drop(&mut self, rep: Resource<CommandBuffer>) -> wasmtime::anyhow::Result<()> {
        let key = Resource::<GpuCommandBuffer>::new_own(rep.rep());
        let cmd_buf = self.table.delete(key)?;
        unsafe {
            self.vk_device()
                .free_command_buffers(cmd_buf.pool, &[cmd_buf.cmd]);
        }
        Ok(())
    }
}
