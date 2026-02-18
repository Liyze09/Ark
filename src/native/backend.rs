#![allow(dead_code, unused)]
pub mod transfer;
pub mod state;

use anyhow::{anyhow, Error, Result};
use ash::khr::ray_tracing_pipeline;
use ash::vk::{ExternalMemoryHandleTypeFlags, HANDLE, MemoryGetWin32HandleInfoKHR};
use dashmap::DashMap;
use rayon::prelude::*;
use smallvec::smallvec;
use vulkano::descriptor_set::DescriptorSet;
use vulkano::descriptor_set::allocator::StandardDescriptorSetAllocator;
use vulkano::descriptor_set::layout::{DescriptorSetLayout, DescriptorSetLayoutBinding, DescriptorSetLayoutCreateInfo, DescriptorType};
use vulkano::pipeline::graphics::rasterization::{CullMode, RasterizationState};
use vulkano::pipeline::graphics::viewport::ViewportState;
use vulkano::shader::ShaderStages;
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::thread;
use vulkano::acceleration_structure::AccelerationStructure;
use vulkano::buffer::allocator::{SubbufferAllocator, SubbufferAllocatorCreateInfo};
use vulkano::buffer::{BufferContents, BufferUsage, Subbuffer};
use vulkano::command_buffer::allocator::{
    StandardCommandBufferAllocator, StandardCommandBufferAllocatorCreateInfo,
};
use vulkano::command_buffer::{
    AutoCommandBufferBuilder, CommandBufferUsage, CopyBufferInfo, PrimaryAutoCommandBuffer, PrimaryCommandBufferAbstract
};
use vulkano::command_buffer::{
    ClearColorImageInfo, CommandBufferSubmitInfo, RenderingAttachmentInfo, RenderingInfo,
    SemaphoreSubmitInfo, SubmitInfo,
};
use vulkano::device::physical::PhysicalDevice;
use vulkano::device::{
    Device, DeviceCreateInfo, DeviceExtensions, DeviceFeatures, Queue, QueueCreateInfo, QueueFlags,
};
use vulkano::format::{ClearColorValue, Format};
use vulkano::image::sys::RawImage;
use vulkano::image::view::ImageView;
use vulkano::image::{Image, ImageCreateInfo, ImageLayout, ImageType, ImageUsage};
use vulkano::instance::{Instance, InstanceCreateFlags, InstanceCreateInfo};
use vulkano::memory::allocator::{MemoryAllocator, MemoryTypeFilter, StandardMemoryAllocator};
use vulkano::memory::{
    DedicatedAllocation, DeviceMemory, ExternalMemoryHandleTypes, MemoryAllocateInfo,
    MemoryPropertyFlags, ResourceMemory,
};
use vulkano::pipeline::graphics::GraphicsPipelineCreateInfo;
use vulkano::pipeline::layout::{PipelineDescriptorSetLayoutCreateInfo, PipelineLayoutCreateInfo};
use vulkano::pipeline::ray_tracing::{RayTracingPipeline, RayTracingPipelineCreateInfo};
use vulkano::pipeline::{GraphicsPipeline, Pipeline, PipelineLayout};
use vulkano::sync::fence::{Fence, FenceCreateInfo};
use vulkano::sync::semaphore::{
    ExternalSemaphoreHandleType, ExternalSemaphoreHandleTypes, Semaphore, SemaphoreCreateInfo,
};
use vulkano::{DeviceSize, NonExhaustive, Version, VulkanLibrary, VulkanObject};
use windows_sys::Win32::Foundation::CloseHandle;
use crate::backend::state::RenderStateManager;
use crate::terrain::BuiltSections;
use crate::texture::AtlasManager;

#[derive(Clone)]
pub struct VkBackend {
    raw: Raw,
    queues: Queues,
    device: Arc<Device>,
    device_properties: DeviceProperties,
    memory_allocator: Arc<StandardMemoryAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    size: Arc<RwLock<(u32, u32)>>,
    memory_type_index: u32,
    render_target: RenderTargetWrapper,
    semaphore: SharedSemaphore,
    host_subbuffer_alloc: Arc<Mutex<SubbufferAllocator>>,
    exported_textures: Arc<DashMap<HANDLE, ExportedImage>>,
    atlas_manager: Arc<AtlasManager>,
    terrain: Arc<BuiltSections>,
    //graphics_pipeline: Arc<GraphicsPipeline>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    descriptor_sets: Vec<Arc<DescriptorSet>>,
    state_manager: RenderStateManager,
}

impl VkBackend {
    pub fn new() -> Result<VkBackend> {
        let library = VulkanLibrary::new()
            .map_err(|e| anyhow!("backend.rs:error in loading VulkanLibrary: {:?}", e))?;
        let instance = Instance::new(
            library,
            InstanceCreateInfo {
                application_name: Some("nexus-native".to_string()),
                flags: InstanceCreateFlags::ENUMERATE_PORTABILITY,
                ..Default::default()
            },
        )?;
        let physical_devices = instance.enumerate_physical_devices()?;
        let mut score = 0u32;
        let mut device = None;
        let mut dp = None;
        for physical_device in physical_devices {
            let s = check_physical_device(physical_device.clone());
            if s.graphics && s.score > score {
                score = s.score;
                device = Some(physical_device);
                dp = Some(s);
            }
        }
        if device.is_none() {
            return Err(anyhow!("no suitable device found"));
        }
        let physical_device = device.unwrap();
        let device_properties = dp.unwrap();
        let queue_family_index = physical_device
            .queue_family_properties()
            .iter()
            .enumerate()
            .position(|(_queue_family_index, queue_family_properties)| {
                queue_family_properties
                    .queue_flags
                    .contains(QueueFlags::GRAPHICS)
            })
            .ok_or(anyhow!("no suitable queue family"))? as u32;
        let (device, mut queues) = Device::new(
            physical_device.clone(),
            DeviceCreateInfo {
                queue_create_infos: vec![QueueCreateInfo {
                    queue_family_index,
                    ..Default::default()
                }],
                enabled_extensions: DeviceExtensions {
                    khr_external_memory: true,
                    khr_external_semaphore: true,
                    #[cfg(windows)]
                    khr_external_memory_win32: true,
                    #[cfg(windows)]
                    khr_external_semaphore_win32: true,
                    #[cfg(unix)]
                    khr_external_memory_fd: true,
                    #[cfg(unix)]
                    khr_external_semaphore_fd: true,
                    khr_acceleration_structure: device_properties.ray_trace,
                    khr_ray_tracing_pipeline: device_properties.ray_trace,
                    khr_ray_query: device_properties.ray_trace,
                    khr_synchronization2: true,
                    ext_mesh_shader: true,
                    ext_descriptor_indexing: true,
                    ..Default::default()
                },
                enabled_features: DeviceFeatures {
                    dynamic_rendering: true,
                    ray_tracing_pipeline: device_properties.ray_trace,
                    ray_query: device_properties.ray_trace,
                    acceleration_structure: device_properties.ray_trace,
                    mesh_shader: true,
                    task_shader: true,
                    ..Default::default()
                },
                ..Default::default()
            },
        )?;
        let memory_type_index = physical_device
            .memory_properties()
            .memory_types
            .iter()
            .position(|i| i.property_flags.contains(MemoryPropertyFlags::DEVICE_LOCAL))
            .ok_or(anyhow!("no suitable device memory type"))?
            as u32;
        let queues = Queues::select(queues)?;
        let memory_allocator = Arc::new(StandardMemoryAllocator::new_default(device.clone()));
        let command_buffer_allocator = Arc::new(StandardCommandBufferAllocator::new(
            device.clone(),
            StandardCommandBufferAllocatorCreateInfo::default(),
        ));
        let raw = Raw::load(instance, device.clone())
            .map_err(|e| anyhow!("backend.rs:error in loading ash objects: {:?}", e))?;
        let gl_ready = Semaphore::new(
            device.clone(),
            SemaphoreCreateInfo {
                #[cfg(windows)]
                export_handle_types: ExternalSemaphoreHandleTypes::OPAQUE_WIN32,
                #[cfg(unix)]
                export_handle_types: ExternalSemaphoreHandleTypes::OPAQUE_FD,
                ..Default::default()
            },
        )
        .map_err(|err| anyhow!("backend.rs:error in creating semaphore gl_ready: {:?}", err))?;
        let gl_complete = Semaphore::new(
            device.clone(),
            SemaphoreCreateInfo {
                #[cfg(windows)]
                export_handle_types: ExternalSemaphoreHandleTypes::OPAQUE_WIN32,
                #[cfg(unix)]
                export_handle_types: ExternalSemaphoreHandleTypes::OPAQUE_FD,
                ..Default::default()
            },
        )
        .map_err(|err| {
            anyhow!(
                "backend.rs:error in creating semaphore gl_complete: {:?}",
                err
            )
        })?;
        /*let ray_tracing_pipeline = RayTracingPipeline::new(
        device.clone(),
        None,
        {
            let mut info = RayTracingPipelineCreateInfo::layout(PipelineLayout::new(
                device.clone(),
                PipelineLayoutCreateInfo::default(),
            ).map_err(|err| anyhow!("backend.rs:error in creating ray tracing pipeline layout: {:?}", err))?);
            info.max_pipeline_ray_recursion_depth = 8;
            info
        },
        ).map_err(|err| anyhow!("backend.rs:error in creating ray tracing pipeline: {:?}", err))?;*/
        /*let graphics_pipeline = GraphicsPipeline::new(device.clone(), None,
            {
                let mut info = GraphicsPipelineCreateInfo
                    ::layout(PipelineLayout::new(
                        device.clone(), 
                        PipelineLayoutCreateInfo {
                            set_layouts: vec![
                                DescriptorSetLayout::new(
                                    device.clone(),
                                    DescriptorSetLayoutCreateInfo {
                                        bindings: BTreeMap::from([
                                            (0, DescriptorSetLayoutBinding {
                                                stages: ShaderStages::TASK | ShaderStages::MESH | ShaderStages::FRAGMENT,
                                                ..DescriptorSetLayoutBinding::descriptor_type(DescriptorType::UniformBufferDynamic)
                                            }),
                                            (1, DescriptorSetLayoutBinding{ 
                                                stages: ShaderStages::TASK | ShaderStages::MESH | ShaderStages::FRAGMENT,
                                                ..DescriptorSetLayoutBinding::descriptor_type(DescriptorType::CombinedImageSampler)
                                            })
                                        ]),
                                        ..Default::default()
                                    },
                                ).map_err(|err| anyhow!("backend.rs:error in creating descriptor set layout: {:?}", err))?,
                            ],
                            ..Default::default()
                        })
                    .map_err(|err| anyhow!("backend.rs:error in creating graphics pipeline layout: {:?}", err))?);
                info.rasterization_state = Some(RasterizationState {
                    cull_mode: CullMode::Back,
                    ..Default::default()
                });
                info.viewport_state = Some(ViewportState::default());
                info
            },
        ).map_err(|err| anyhow!("backend.rs:error in creating graphics pipeline: {:?}", err))?;*/
        let descriptor_set_allocator = Arc::new(StandardDescriptorSetAllocator::new(device.clone(), Default::default()));
        let descriptor_sets = vec![];
        
        Ok(VkBackend {
            raw,
            queues,
            device,
            device_properties,
            memory_allocator: memory_allocator.clone(),
            descriptor_set_allocator,
            command_buffer_allocator,
            size: Arc::new(RwLock::new((0, 0))),
            memory_type_index,
            render_target: RenderTargetWrapper::default(),
            semaphore: SharedSemaphore {
                handle_gl_ready: gl_ready
                    .export_win32_handle(ExternalSemaphoreHandleType::OpaqueWin32)
                    .map_err(|err| {
                        anyhow!(
                            "backend.rs:error in exporting semaphore gl_ready: {:?}",
                            err
                        )
                    })?,
                gl_ready: Arc::new(gl_ready),
                handle_gl_complete: gl_complete
                    .export_win32_handle(ExternalSemaphoreHandleType::OpaqueWin32)
                    .map_err(|err| {
                        anyhow!(
                            "backend.rs:error in exporting semaphore gl_complete: {:?}",
                            err
                        )
                    })?,
                gl_complete: Arc::new(gl_complete),
            },
            host_subbuffer_alloc: Arc::new(Mutex::new(SubbufferAllocator::new(
                memory_allocator.clone(),
                SubbufferAllocatorCreateInfo {
                    buffer_usage: BufferUsage::TRANSFER_SRC,
                    memory_type_filter: MemoryTypeFilter::PREFER_HOST,
                    ..Default::default()
                },
            ))),
            exported_textures: Arc::new(DashMap::new()),
            //graphics_pipeline,
            atlas_manager: Arc::new(AtlasManager::new()),
            terrain: Arc::new(BuiltSections::new(memory_allocator)),
            descriptor_sets,
            state_manager: RenderStateManager::default(),
        })
    }

    pub fn render(&self) -> Result<Arc<Fence>> {
        let target = self.render_target.get_value()?;
        let mut builder = self.allocate_command_buffer(
            QueueType::Graphics,
            CommandBufferUsage::OneTimeSubmit,
        )?;
        builder
            .clear_color_image(ClearColorImageInfo {
                clear_value: ClearColorValue::Float([0.0, 0.0, 1.0, 0.0]),
                image_layout: ImageLayout::General,
                ..ClearColorImageInfo::image(target.image.clone())
            })?
            /*.begin_rendering(RenderingInfo {
                color_attachments: vec![Some(RenderingAttachmentInfo::image_view(target.image_view))],
                ..Default::default()
            })?
            .bind_pipeline_graphics(self.graphics_pipeline.clone())?
            .bind_descriptor_sets(
                vulkano::pipeline::PipelineBindPoint::Graphics,
                self.graphics_pipeline.layout().clone(),
                0,
                self.descriptor_sets.clone(),
            )?;
        builder.end_rendering()?*/;
        let command_buffer = builder.build()?;
        let fence = Arc::new(Fence::from_pool(self.device().clone())?);
        self.queues.graphics.with(|mut queue| -> Result<()> {
            unsafe {
                Ok(queue.submit(
                    &[SubmitInfo {
                        wait_semaphores: vec![SemaphoreSubmitInfo::new(
                            self.semaphore.gl_complete.clone(),
                        )],
                        command_buffers: vec![CommandBufferSubmitInfo::new(command_buffer)],
                        signal_semaphores: vec![SemaphoreSubmitInfo::new(
                            self.semaphore.gl_ready.clone(),
                        )],
                        ..Default::default()
                    }],
                    Some(&fence),
                )?)
            }
        })?;
        Ok(fence)
    }

    #[inline]
    pub fn resize(&self, size: (u32, u32)) {
        let mut guard = self.size.write().unwrap();
        guard.0 = size.0;
        guard.1 = size.1;
    }

    pub fn update(&self) -> Result<()> {
        let size = self
            .size
            .read()
            .map_err(|err| anyhow!("backend.rs:error in creating RawImage: {:?}", err))?;
        if let Ok(target) = self.render_target.get_value() {
            drop(self.exported_textures.remove(&target.handle));
        }
        let (image, size, device_memory, handle) = self.create_external_texture(
            ImageUsage::COLOR_ATTACHMENT
                | ImageUsage::SAMPLED
                | ImageUsage::STORAGE
                | ImageUsage::TRANSFER_DST
                | ImageUsage::TRANSFER_SRC,
            *size,
            1
        )?;
        let view = ImageView::new_default(image.clone())
            .map_err(|err| anyhow!("backend.rs:error in creating image view: {:?}", err))?;
        self.render_target
            .reset(device_memory, image, handle, view, size)
            .map_err(|err| anyhow!("backend.rs:error in resetting RenderTarget: {:?}", err))?;
        Ok(())
    }

    #[inline]
    pub fn target(&self) -> &RenderTargetWrapper {
        &self.render_target
    }

    #[inline]
    pub fn semaphore(&self) -> &SharedSemaphore {
        &self.semaphore
    }

    #[inline]
    pub fn queue(&self) -> &Queues {
        &self.queues
    }

    #[inline]
    pub fn memory_allocator(&self) -> &Arc<StandardMemoryAllocator> {
        &self.memory_allocator
    }

    #[inline]
    pub fn command_buffer_allocator(&self) -> &Arc<StandardCommandBufferAllocator> {
        &self.command_buffer_allocator
    }

    #[inline]
    pub fn device(&self) -> &Arc<Device> {
        &self.device
    }

    #[inline]
    pub fn physical_device(&self) -> &Arc<PhysicalDevice> {
        self.device.physical_device()
    }

    #[inline]
    pub fn instance(&self) -> &Arc<Instance> {
        self.device.instance()
    }

    #[inline]
    pub fn device_properties(&self) -> &DeviceProperties {
        &self.device_properties
    }

    pub fn create_external_texture(
        &self,
        usage: ImageUsage,
        size: (u32, u32),
        mip_levels: u32,
    ) -> Result<(Arc<Image>, DeviceSize, Arc<DeviceMemory>, HANDLE)> {
        let create_info = ImageCreateInfo {
            image_type: ImageType::Dim2d,
            format: Format::R8G8B8A8_UNORM,
            extent: [size.0, size.1, 1],
            mip_levels,
            usage,
            external_memory_handle_types: ExternalMemoryHandleTypes::OPAQUE_WIN32,
            ..Default::default()
        };
        let raw_image = RawImage::new(self.device.clone(), create_info).map_err(|err| {
            anyhow!(
                "backend.rs:create_external_memory:error in creating RawImage: {:?}",
                err
            )
        })?;
        let size = raw_image.memory_requirements()[0].layout.size();
        let memory = self
            .memory_allocator
            .allocate_dedicated(
                self.memory_type_index,
                size,
                Some(DedicatedAllocation::Image(&raw_image)),
                #[cfg(windows)]
                ExternalMemoryHandleTypes::OPAQUE_WIN32,
                #[cfg(unix)]
                ExternalMemoryHandleTypes::OPAQUE_FD,
            )
            .map_err(|err| {
                anyhow!(
                    "backend.rs:create_external_memory:error in allocating device memory: {:?}",
                    err
                )
            })?;
        let device_memory = memory.device_memory.clone();
        let allocation =
            unsafe { ResourceMemory::from_allocation(self.memory_allocator.clone(), memory) };
        let image = Arc::new(raw_image.bind_memory([allocation]).map_err(|(err, _, _)| {
            anyhow!(
                "backend.rs:create_external_memory:error in binding memory and image: {:?}",
                err
            )
        })?);
        let vk_memory = device_memory.handle();
        let info = MemoryGetWin32HandleInfoKHR {
            memory: vk_memory,
            handle_type: ExternalMemoryHandleTypeFlags::OPAQUE_WIN32,
            ..Default::default()
        };
        let handle = unsafe {
            self.raw.ext_device.get_memory_win32_handle(&info)
                .map_err(|err| anyhow!("backend.rs:create_external_memory:error in getting external memory handle: {:?}", err))?
        };
        self.exported_textures.insert(
            handle,
            ExportedImage {
                image: image.clone(),
                image_view: None,
                memory: device_memory.clone(),
                handle,
                size,
            },
        );
        Ok((image, size, device_memory, handle))
    }

    pub fn get_texture_size_by_handle(&self, handle: HANDLE) -> Result<DeviceSize> {
        let exported_texture = self
            .exported_textures
            .get(&handle)
            .ok_or_else(|| anyhow!("Texture with handle {:?} not found", handle))?;

        Ok(exported_texture.size)
    }

    pub fn get_exported_image_by_handle(&self, handle: HANDLE) -> Result<ExportedImage> {
        let exported_texture = self
            .exported_textures
            .get(&handle)
            .ok_or_else(|| anyhow!("Texture with handle {:?} not found", handle))?;

        Ok(exported_texture.clone())
    }

    /// Synchronize atlas information with sprite mappings
    pub fn sync_atlas(
        &self,
        texture: Arc<Image>,
        atlas_name: String,
        sprites: HashMap<String, crate::texture::SpriteInfo>,
    ) {
        self.atlas_manager.sync_atlas(texture, atlas_name, sprites);
    }

    pub fn atlas_manager(&self) -> &Arc<AtlasManager> {
        &self.atlas_manager
    }

    pub fn terrain_manager(&self) -> &Arc<BuiltSections> {
        &self.terrain
    }

    pub fn state_manager(&self) -> &RenderStateManager {
        &self.state_manager
    }

    #[inline]
    pub fn allocate_command_buffer(&self, queue_type: QueueType, usage: CommandBufferUsage) -> Result<AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>> {
        Ok(AutoCommandBufferBuilder::primary(
            self.command_buffer_allocator.clone(), 
            match queue_type {
                QueueType::Graphics => self.queues.graphics.queue_family_index(),
                QueueType::Compute => self.queues.compute.queue_family_index(),
                QueueType::Transfer => self.queues.transfer.queue_family_index()
            }, usage)?)
    }
}

fn check_physical_device(physical_device: Arc<PhysicalDevice>) -> DeviceProperties {
    let mut score = 0u32;
    let mut graphics = true;
    let mut ray_trace = false;
    if physical_device.api_version() >= Version::V1_4 {
        score += 1;
    }
    if physical_device.supported_features().ray_tracing_pipeline
        || !physical_device
            .supported_extensions()
            .khr_deferred_host_operations
    {
        score += 1;
        ray_trace = true;
    }
    if physical_device.api_version() < Version::V1_3
        || !physical_device.supported_extensions().khr_external_memory
        || !physical_device.supported_features().dynamic_rendering
    {
        graphics = false;
    }
    DeviceProperties {
        graphics,
        ray_trace,
        score,
    }
}

impl Drop for VkBackend {
    fn drop(&mut self) {
        unsafe { self.device.wait_idle().expect("Error in closing VkBackend") };
        let handles: Vec<HANDLE> = self.exported_textures.iter().map(|e| *e.key()).collect();
        self.exported_textures.clear();
        for handle in handles {
            unsafe {
                // CloseHandle(handle as _);
            }
        }
    }
}

#[derive(Clone)]
struct Raw {
    vk_instance: ash::Instance,
    ext_device: ash::khr::external_memory_win32::Device,
    vk_device: ash::Device,
}

impl Raw {
    fn load(instance: Arc<Instance>, device: Arc<Device>) -> Result<Self> {
        let vk_raw_device = device.handle();
        let vk_raw_instance = instance.handle();
        let fns = instance.fns();
        let vk_instance = ash::Instance::from_parts_1_3(
            vk_raw_instance,
            fns.v1_0.clone(),
            fns.v1_1.clone(),
            fns.v1_3.clone(),
        );
        let fns = device.fns();
        let vk_device = ash::Device::from_parts_1_3(
            vk_raw_device,
            fns.v1_0.clone(),
            fns.v1_1.clone(),
            fns.v1_2.clone(),
            fns.v1_3.clone(),
        );

        let ext_device = ash::khr::external_memory_win32::Device::new(&vk_instance, &vk_device);
        Ok(Raw {
            vk_device,
            vk_instance,
            ext_device,
        })
    }
}

#[derive(Clone, Default)]
pub struct RenderTargetWrapper(Arc<RwLock<Option<RenderTarget>>>);
#[derive(Clone)]
pub struct RenderTarget {
    pub memory: Arc<DeviceMemory>,
    pub size: u64,
    pub handle: HANDLE,
    pub image: Arc<Image>,
    pub image_view: Arc<ImageView>,
}

pub enum QueueType {
    Graphics,
    Compute,
    Transfer,
}

#[derive(Clone)]
pub struct Queues {
    pub graphics: Arc<Queue>,
    pub compute: Arc<Queue>,
    pub transfer: Arc<Queue>,
}

impl Queues {
    pub fn select(queues: impl ExactSizeIterator<Item = Arc<Queue>>) -> Result<Self> {
        let mut graphics = None;
        let mut compute = None;
        let mut transfer = None;

        let mut queue_infos = Vec::new();
        for queue in queues {
            let props = queue.device().physical_device().queue_family_properties()
                [queue.queue_family_index() as usize]
                .clone();
            let index = queue.queue_family_index();
            queue_infos.push((queue, props.queue_flags, index));
        }

        let mut used_families = Vec::new();

        for (queue, flags, family_index) in &queue_infos {
            if flags.contains(QueueFlags::GRAPHICS) && graphics.is_none() {
                graphics = Some(queue.clone());
                used_families.push(*family_index);
                break;
            }
        }

        for (queue, flags, family_index) in &queue_infos {
            if flags.contains(QueueFlags::COMPUTE)
                && compute.is_none()
                && !flags.contains(QueueFlags::GRAPHICS)
                && !used_families.contains(family_index)
            {
                compute = Some(queue.clone());
                used_families.push(*family_index);
                break;
            }
        }

        if compute.is_none() {
            for (queue, flags, family_index) in &queue_infos {
                if flags.contains(QueueFlags::COMPUTE) && compute.is_none() {
                    compute = Some(queue.clone());
                    if !used_families.contains(family_index) {
                        used_families.push(*family_index);
                    }
                    break;
                }
            }
        }

        for (queue, flags, family_index) in &queue_infos {
            if flags.contains(QueueFlags::TRANSFER)
                && transfer.is_none()
                && !flags.contains(QueueFlags::GRAPHICS)
                && !flags.contains(QueueFlags::COMPUTE)
                && !used_families.contains(family_index)
            {
                transfer = Some(queue.clone());
                used_families.push(*family_index);
                break;
            }
        }

        if transfer.is_none() {
            for (queue, flags, family_index) in &queue_infos {
                if flags.contains(QueueFlags::TRANSFER) && transfer.is_none() {
                    transfer = Some(queue.clone());
                    if !used_families.contains(family_index) {
                        used_families.push(*family_index);
                    }
                    break;
                }
            }
        }
        let graphics = graphics.ok_or(anyhow!("no graphics queue found"))?.clone();
        Ok(Queues {
            graphics: graphics.clone(),
            compute: compute.ok_or(anyhow!("no compute queue found"))?.clone(),
            transfer: transfer.unwrap_or(graphics).clone(),
        })
    }
}

#[derive(Clone)]
pub struct SharedSemaphore {
    pub gl_ready: Arc<Semaphore>,
    pub gl_complete: Arc<Semaphore>,
    pub handle_gl_ready: HANDLE,
    pub handle_gl_complete: HANDLE,
}

impl RenderTargetWrapper {
    pub fn destroy(&self) -> Result<()> {
        let mut value = self.0.write().map_err(|err| anyhow!("{}", err))?;
        let value = value.take();
        if let Some(value) = value {
            let handle = value.handle;
            drop(value);
            unsafe {
                CloseHandle(handle as _);
            }
        }
        Ok(())
    }

    pub fn get_value(&self) -> Result<RenderTarget> {
        self.0
            .read()
            .map_err(|err| anyhow!("{}", err))?
            .clone()
            .ok_or(anyhow!("backend.rs:render target not found"))
    }

    pub fn reset(
        &self,
        memory: Arc<DeviceMemory>,
        image: Arc<Image>,
        handle: HANDLE,
        image_view: Arc<ImageView>,
        size: u64,
    ) -> Result<()> {
        let mut value = self.0.write().map_err(|err| anyhow!("{}", err))?;
        *value = Some(RenderTarget {
            memory,
            handle,
            image,
            image_view,
            size,
        });
        Ok(())
    }
}

#[derive(Clone)]
pub struct ExportedImage {
    pub image: Arc<Image>,
    pub image_view: Option<Arc<ImageView>>,
    pub memory: Arc<DeviceMemory>,
    pub handle: HANDLE,
    pub size: DeviceSize,
}

#[derive(Clone)]
pub struct DeviceProperties {
    pub graphics: bool,
    pub ray_trace: bool,
    pub score: u32,
}
