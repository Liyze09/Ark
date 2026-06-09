use std::{borrow::Cow, collections::HashMap, ffi::CStr};

use vulkanalia::{
    Entry, Instance,
    loader::{LIBRARY, LibloadingLoader},
    vk::{self, DeviceV1_0, EntryV1_0, HasBuilder, InstanceV1_0},
};
use vulkanalia_vma::Allocator;
use vulkanalia_vma::vma::VmaAllocator;
use wasmtime::component::Resource;
use wasmtime_wasi::ResourceTable;

use ark_vk_binding::binding::ark::gpu::{
    buffer::Host as BufHost, buffer::HostBuffer as BufResource, command_buffer::Host as CmdHost,
    command_buffer::HostCommandBufferBuilder as CmdBuilder, descriptor::Host as DescHost,
    pipeline::Host as PipeHost, queue::Host as QueueHost, queue::HostQueue,
    shader::Host as ShaderHost,
};
use ark_vk_binding::{VkContextOwned, VkContextView};

// ── Test infrastructure ───────────────────────────────────────────────

struct VkTestCtx {
    _entry: Entry,
    _instance: Instance,
    _device: vulkanalia::Device,
    owned: VkContextOwned,
}

impl VkTestCtx {
    fn new() -> Self {
        let loader =
            unsafe { LibloadingLoader::new(LIBRARY) }.expect("failed to create Vulkan loader");
        let entry = unsafe { Entry::new(loader) }.expect("Entry creation");

        let app_info = vk::ApplicationInfo::builder()
            .application_name(b"ark-runtime-test\0")
            .api_version(vk::make_version(1, 3, 0));

        let layers: Vec<&CStr> = if has_validation_layer(&entry) {
            vec![c"VK_LAYER_KHRONOS_validation"]
        } else {
            vec![]
        };
        let layer_ptrs: Vec<*const i8> = layers.iter().map(|l| l.as_ptr()).collect();

        let extensions = vec![
            vk::KHR_GET_PHYSICAL_DEVICE_PROPERTIES2_EXTENSION
                .name
                .as_ptr(),
        ];

        let instance_info = vk::InstanceCreateInfo::builder()
            .application_info(&app_info)
            .enabled_layer_names(&layer_ptrs)
            .enabled_extension_names(&extensions);

        let instance =
            unsafe { entry.create_instance(&instance_info, None) }.expect("create instance");

        let pdevices =
            unsafe { instance.enumerate_physical_devices() }.expect("enumerate pdevices");
        let (pdevice, graphics_qf, compute_qf, transfer_qf) =
            pick_physical_device(&instance, &pdevices);

        let mut vk12 = vk::PhysicalDeviceVulkan12Features::builder()
            .descriptor_indexing(true)
            .descriptor_binding_partially_bound(true)
            .runtime_descriptor_array(true)
            .buffer_device_address(true);
        let mut vk13 = vk::PhysicalDeviceVulkan13Features::builder().dynamic_rendering(true);

        let queue_priorities = [1.0f32];
        let mut unique_qfs = vec![graphics_qf, compute_qf, transfer_qf];
        unique_qfs.sort();
        unique_qfs.dedup();

        let queue_infos: Vec<_> = unique_qfs
            .iter()
            .map(|&qf| {
                vk::DeviceQueueCreateInfo::builder()
                    .queue_family_index(qf)
                    .queue_priorities(&queue_priorities)
                    .build()
            })
            .collect();

        let device = unsafe {
            instance.create_device(
                pdevice,
                &vk::DeviceCreateInfo::builder()
                    .queue_create_infos(&queue_infos)
                    .push_next(&mut vk12)
                    .push_next(&mut vk13),
                None,
            )
        }
        .expect("create device");

        let graphics_queue = unsafe { device.get_device_queue(graphics_qf, 0) };
        let compute_queue = unsafe { device.get_device_queue(compute_qf, 0) };
        let transfer_queue = unsafe { device.get_device_queue(transfer_qf, 0) };

        let alloc_opts = vulkanalia_vma::AllocatorOptions::new(&instance, &device, pdevice);
        let vma_wrapper = unsafe { Allocator::new(&alloc_opts) }.expect("VMA create");
        // Allocator is #[repr(transparent)] over VmaAllocator
        let vma: VmaAllocator = unsafe { std::mem::transmute_copy(&vma_wrapper) };
        // Prevent Drop from destroying the allocator (VkContextOwned owns it now)
        std::mem::forget(vma_wrapper);

        let owned = unsafe {
            VkContextOwned::new(
                instance.handle(),
                device.handle(),
                *device.commands(),
                vma,
                graphics_queue,
                compute_queue,
                transfer_queue,
                graphics_qf,
                compute_qf,
                transfer_qf,
            )
        };

        Self {
            _entry: entry,
            _instance: instance,
            _device: device,
            owned,
        }
    }

    fn ctx_view<'a>(
        owned: &'a VkContextOwned,
        table: &'a mut ResourceTable,
        files: &'a HashMap<String, Cow<'static, [u8]>>,
    ) -> VkContextView<'a> {
        VkContextView {
            owned,
            table,
            files,
        }
    }
}

fn has_validation_layer(entry: &Entry) -> bool {
    let Ok(props) = (unsafe { entry.enumerate_instance_layer_properties() }) else {
        return false;
    };
    let target = c"VK_LAYER_KHRONOS_validation";
    props
        .iter()
        .any(|lp| unsafe { CStr::from_ptr(lp.layer_name.as_ptr()) } == target)
}

fn pick_physical_device(
    instance: &Instance,
    devices: &[vk::PhysicalDevice],
) -> (vk::PhysicalDevice, u32, u32, u32) {
    let mut fallback = None;
    for &pd in devices {
        let props = unsafe { instance.get_physical_device_properties(pd) };
        let qf_props = unsafe { instance.get_physical_device_queue_family_properties(pd) };

        let graphics = qf_props
            .iter()
            .position(|p| p.queue_flags.contains(vk::QueueFlags::GRAPHICS));
        let compute = qf_props
            .iter()
            .position(|p| p.queue_flags.contains(vk::QueueFlags::COMPUTE));
        let transfer = qf_props.iter().position(|p| {
            p.queue_flags.contains(vk::QueueFlags::TRANSFER)
                && !p.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                && !p.queue_flags.contains(vk::QueueFlags::COMPUTE)
        });

        if let (Some(g), Some(c)) = (graphics, compute) {
            let t = transfer.unwrap_or(g);
            let result = (pd, g as u32, c as u32, t as u32);
            if props.device_type == vk::PhysicalDeviceType::DISCRETE_GPU {
                return result;
            }
            fallback.get_or_insert(result);
        }
    }
    fallback.expect("no suitable Vulkan device found")
}

// ── Helper: build a compute pipeline with one storage buffer binding ──

struct ComputePipelineSet {
    pl: wasmtime::component::Resource<ark_vk_binding::binding::ark::gpu::pipeline::PipelineLayout>,
    cp: wasmtime::component::Resource<ark_vk_binding::binding::ark::gpu::pipeline::ComputePipeline>,
    dsl: wasmtime::component::Resource<
        ark_vk_binding::binding::ark::gpu::descriptor::DescriptorSetLayout,
    >,
    set:
        wasmtime::component::Resource<ark_vk_binding::binding::ark::gpu::descriptor::DescriptorSet>,
}

fn build_compute_pipeline(
    v: &mut VkContextView<'_>,
    spirv: &[u32],
    entry: &str,
) -> ComputePipelineSet {
    use ark_vk_binding::binding::ark::gpu::{
        descriptor::{
            DescriptorBinding, DescriptorBindingFlags, DescriptorPoolCreateFlags, DescriptorType,
            PoolSize,
        },
        pipeline::DescriptorSetInfo,
    };
    use wasmtime::component::Resource;

    let shader = v.shader_from_bytes(spirv.to_vec()).expect("create shader");

    let bindings = vec![DescriptorBinding {
        binding: 0,
        descriptor_type: DescriptorType::StorageBuffer,
        descriptor_count: 1,
        stage_flags: vk::ShaderStageFlags::COMPUTE.bits(),
        binding_flags: DescriptorBindingFlags::empty(),
    }];
    let dsl = v.create_descriptor_set_layout(bindings).expect("dsl");

    let pool = v
        .create_descriptor_pool(
            1,
            vec![PoolSize {
                descriptor_type: DescriptorType::StorageBuffer,
                descriptor_count: 1,
            }],
            DescriptorPoolCreateFlags::empty(),
        )
        .expect("pool");
    // Borrow dsl for allocation and layout creation
    let dsl_alloc = Resource::new_borrow(dsl.rep());
    let set = v
        .allocate_descriptor_set(pool, dsl_alloc, vec![])
        .expect("allocate set");

    // Borrow dsl for pipeline layout
    let dsl_layout = Resource::new_borrow(dsl.rep());
    let pl = v
        .create_pipeline_layout(
            vec![DescriptorSetInfo {
                layout: dsl_layout,
                set: 0,
            }],
            vec![],
        )
        .expect("pipeline layout");

    // Borrow pl and shader for pipeline creation
    let pl_cp = Resource::new_borrow(pl.rep());
    let shader_cp = Resource::new_borrow(shader.rep());
    let cp = v
        .create_compute_pipeline(pl_cp, shader_cp, entry.into())
        .expect("compute pipeline");

    ComputePipelineSet { pl, cp, dsl, set }
}

// ── Test 1: compute shader f32 buffer add ─────────────────────────────

#[test]
fn compute_add_f32() {
    let ctx = VkTestCtx::new();
    let mut table = ResourceTable::new();
    let files: HashMap<String, Cow<'static, [u8]>> = HashMap::new();
    let v = &mut VkTestCtx::ctx_view(&ctx.owned, &mut table, &files);

    const N: u64 = 64;

    // GLSL → SPIR-V
    let spirv = compile_glsl(
        r"#version 460
        layout(local_size_x = 64) in;
        layout(set = 0, binding = 0) buffer Data { float v[]; };
        void main() {
            uint idx = gl_GlobalInvocationID.x;
            v[idx] = v[idx] + 1.0;
        }",
        "main",
        shaderc::ShaderKind::Compute,
    );

    // Input buffer
    let data_in: Vec<f32> = (0..N).map(|i| i as f32).collect();
    use ark_vk_binding::binding::ark::gpu::{
        buffer::{BufferCreateInfo, BufferUsage},
        memory::{AllocateInfo, MemoryType},
    };

    let create_info = BufferCreateInfo {
        size: N * 4,
        usage: BufferUsage::STORAGE_BUFFER | BufferUsage::TRANSFER_SRC | BufferUsage::TRANSFER_DST,
        sharing_mode: None,
    };
    let alloc = AllocateInfo {
        memory_type: MemoryType::PREFER_DEVICE | MemoryType::HOST_SEQUENTIAL_WRITE,
    };
    let data_bytes =
        unsafe { std::slice::from_raw_parts(data_in.as_ptr() as *const u8, (N as usize) * 4) }
            .to_vec();
    let buf = v
        .buffer_from_data(create_info, alloc, data_bytes)
        .expect("create buffer");

    let cps = build_compute_pipeline(v, &spirv, "main");

    // Write descriptor
    use ark_vk_binding::binding::ark::gpu::descriptor::{
        BufferDescriptorInfo, DescriptorType, DescriptorWrite,
    };
    use wasmtime::component::Resource;
    v.write_descriptor_set(
        Resource::new_borrow(cps.set.rep()),
        vec![DescriptorWrite {
            binding: 0,
            dst_array_element: 0,
            descriptor_count: 1,
            descriptor_type: DescriptorType::StorageBuffer,
            buffer_info: Some(BufferDescriptorInfo {
                buffer: Resource::new_borrow(buf.rep()),
                offset: 0,
                range: N * 4,
            }),
            image_info: None,
        }],
    )
    .expect("write descriptor");

    use ark_vk_binding::binding::ark::gpu::{
        command_buffer::{CommandBufferUsage, PipelineBindPoint},
        core::QueueFamily,
    };

    // Record
    let builder = v.primary_command_buffer(QueueFamily::Compute, CommandBufferUsage::OneTimeSubmit);
    v.bind_compute_pipeline(
        Resource::new_borrow(builder.rep()),
        Resource::new_borrow(cps.cp.rep()),
    )
    .expect("bind cp");
    v.bind_descriptor_sets(
        Resource::new_borrow(builder.rep()),
        PipelineBindPoint::Compute,
        Resource::new_borrow(cps.pl.rep()),
        0,
        vec![Resource::new_borrow(cps.set.rep())],
    )
    .expect("bind ds");
    v.dispatch(
        Resource::new_borrow(builder.rep()),
        (N as u32).div_ceil(64),
        1,
        1,
    )
    .expect("dispatch");

    let cb = v.build_command_buffer(builder);
    let q = v.compute();
    v.submit(
        Resource::new_borrow(q.rep()),
        vec![Resource::new_borrow(cb.rep())],
        vec![],
        vec![],
        None,
    )
    .expect("submit");
    v.wait_idle(Resource::new_borrow(q.rep()))
        .expect("wait idle");

    // Verify
    let result_bytes = v.read(buf, 0, N * 4).expect("read buffer");
    let result_f32: &[f32] =
        unsafe { std::slice::from_raw_parts(result_bytes.as_ptr() as *const f32, N as usize) };

    for i in 0..N as usize {
        let expected = i as f32 + 1.0;
        assert!(
            (result_f32[i] - expected).abs() < 0.005,
            "[{i}] expected {expected}, got {}",
            result_f32[i]
        );
    }
    println!(
        "✓ compute_add_f32: {N} elements OK (first={:.1}, last={:.1})",
        result_f32[0],
        result_f32[N as usize - 1]
    );
}

// ── Test 2: compute shader render triangle to PNG ─────────────────────

#[test]
fn render_triangle_to_png() {
    let ctx = VkTestCtx::new();
    let mut table = ResourceTable::new();
    let files: HashMap<String, Cow<'static, [u8]>> = HashMap::new();
    let v = &mut VkTestCtx::ctx_view(&ctx.owned, &mut table, &files);

    const W: u32 = 512;
    const H: u32 = 512;

    // GLSL → SPIR-V
    let spirv = compile_glsl(
        r"#version 460
        layout(local_size_x = 8, local_size_y = 8) in;
        layout(set = 0, binding = 0) buffer OutBuf { uint pixels[]; };

        bool inside_triangle(vec2 p) {
            vec2 a = vec2(256.0, 64.0);
            vec2 b = vec2(64.0, 448.0);
            vec2 c = vec2(448.0, 448.0);
            vec2 v0 = c - a, v1 = b - a, v2 = p - a;
            float d00 = dot(v0, v0), d01 = dot(v0, v1), d11 = dot(v1, v1);
            float d20 = dot(v2, v0), d21 = dot(v2, v1);
            float denom = d00 * d11 - d01 * d01;
            if (abs(denom) < 0.0001) return false;
            float v = (d11 * d20 - d01 * d21) / denom;
            float w = (d00 * d21 - d01 * d20) / denom;
            float u = 1.0 - v - w;
            return u >= 0.0 && v >= 0.0 && w >= 0.0;
        }

        void main() {
            uvec2 pos = gl_GlobalInvocationID.xy;
            if (pos.x >= 512u || pos.y >= 512u) return;
            uint idx = pos.y * 512u + pos.x;
            bool inside = inside_triangle(vec2(pos));
            uint r = inside ? 255u : 30u;
            uint g = inside ? 50u  : 30u;
            uint b = inside ? 50u  : 180u;
            uint a = 255u;
            pixels[idx] = (a << 24) | (b << 16) | (g << 8) | r;
        }",
        "main",
        shaderc::ShaderKind::Compute,
    );

    use ark_vk_binding::binding::ark::gpu::{
        buffer::{BufferCreateInfo, BufferUsage},
        memory::{AllocateInfo, MemoryType},
    };

    let img_size: u64 = (W * H * 4) as u64;
    let create_info = BufferCreateInfo {
        size: img_size,
        usage: BufferUsage::STORAGE_BUFFER | BufferUsage::TRANSFER_SRC | BufferUsage::TRANSFER_DST,
        sharing_mode: None,
    };
    let alloc = AllocateInfo {
        memory_type: MemoryType::PREFER_DEVICE | MemoryType::HOST_RANDOM_ACCESS,
    };
    // buffer_from_data forces HOST_VISIBLE memory; buffer_zeroed does not.
    let out_buf = v
        .buffer_from_data(create_info, alloc, vec![0u8; img_size as usize])
        .expect("create output buffer");

    let cps = build_compute_pipeline(v, &spirv, "main");

    use ark_vk_binding::binding::ark::gpu::descriptor::{
        BufferDescriptorInfo, DescriptorType, DescriptorWrite,
    };
    use wasmtime::component::Resource;
    v.write_descriptor_set(
        Resource::new_borrow(cps.set.rep()),
        vec![DescriptorWrite {
            binding: 0,
            dst_array_element: 0,
            descriptor_count: 1,
            descriptor_type: DescriptorType::StorageBuffer,
            buffer_info: Some(BufferDescriptorInfo {
                buffer: Resource::new_borrow(out_buf.rep()),
                offset: 0,
                range: img_size,
            }),
            image_info: None,
        }],
    )
    .expect("write descriptor");

    use ark_vk_binding::binding::ark::gpu::{
        command_buffer::{CommandBufferUsage, PipelineBindPoint},
        core::QueueFamily,
    };

    let builder = v.primary_command_buffer(QueueFamily::Compute, CommandBufferUsage::OneTimeSubmit);
    v.bind_compute_pipeline(
        Resource::new_borrow(builder.rep()),
        Resource::new_borrow(cps.cp.rep()),
    )
    .expect("bind cp");
    v.bind_descriptor_sets(
        Resource::new_borrow(builder.rep()),
        PipelineBindPoint::Compute,
        Resource::new_borrow(cps.pl.rep()),
        0,
        vec![Resource::new_borrow(cps.set.rep())],
    )
    .expect("bind ds");
    v.dispatch(
        Resource::new_borrow(builder.rep()),
        W.div_ceil(8),
        H.div_ceil(8),
        1,
    )
    .expect("dispatch");

    let cb = v.build_command_buffer(builder);
    let q = v.compute();
    v.submit(
        Resource::new_borrow(q.rep()),
        vec![Resource::new_borrow(cb.rep())],
        vec![],
        vec![],
        None,
    )
    .expect("submit");
    v.wait_idle(Resource::new_borrow(q.rep()))
        .expect("wait idle");

    // Read back
    let result = v.read(out_buf, 0, img_size).expect("read back");

    // Write PNG
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/target/triangle_test.png");
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let file = std::fs::File::create(path).expect("create PNG file");
    let mut encoder = png::Encoder::new(file, W, H);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("PNG header");
    writer.write_image_data(&result).expect("write PNG data");
    drop(writer);
    println!("✓ render_triangle_to_png: written to {path}");
}

// ── Test 3: graphics pipeline colored triangle ────────────────────────

#[test]
fn graphics_pipeline_triangle() {
    let ctx = VkTestCtx::new();
    let mut table = ResourceTable::new();
    let files: HashMap<String, Cow<'static, [u8]>> = HashMap::new();
    let v = &mut VkTestCtx::ctx_view(&ctx.owned, &mut table, &files);

    const W: u32 = 512;
    const H: u32 = 512;

    // ── GLSL shaders ────────────────────────────────────────────────

    let vert_spirv = compile_glsl(
        r"#version 460
        vec2 positions[3] = vec2[](vec2(0.0, -0.5), vec2(0.5, 0.5), vec2(-0.5, 0.5));
        vec3 colors[3]   = vec3[](vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), vec3(0.0, 0.0, 1.0));

        layout(location = 0) out vec3 frag_color;
        void main() {
            gl_Position = vec4(positions[gl_VertexIndex], 0.0, 1.0);
            frag_color = colors[gl_VertexIndex];
        }",
        "main",
        shaderc::ShaderKind::Vertex,
    );

    let frag_spirv = compile_glsl(
        r"#version 460
        layout(location = 0) in vec3 frag_color;
        layout(location = 0) out vec4 out_color;
        void main() {
            out_color = vec4(frag_color, 1.0);
        }",
        "main",
        shaderc::ShaderKind::Fragment,
    );

    // ── WIT: create shader modules, layout, graphics pipeline ──────

    let vert_mod = v.shader_from_bytes(vert_spirv).expect("vert shader module");
    let frag_mod = v.shader_from_bytes(frag_spirv).expect("frag shader module");

    // Empty pipeline layout (no descriptors, no push constants)
    let pl = v
        .create_pipeline_layout(vec![], vec![])
        .expect("pipeline layout");

    use ark_vk_binding::binding::ark::gpu::pipeline::{
        GraphicsPipelineCreateInfo, PrimitiveTopology,
    };

    let gp_info = GraphicsPipelineCreateInfo {
        layout: Resource::new_borrow(pl.rep()),
        vertex_shader: Resource::new_borrow(vert_mod.rep()),
        vertex_entry: "main".into(),
        fragment_shader: Resource::new_borrow(frag_mod.rep()),
        fragment_entry: "main".into(),
        vertex_attributes: vec![], // using gl_VertexIndex
        topology: PrimitiveTopology::TriangleList,
        color_format: vk::Format::R8G8B8A8_UNORM.as_raw() as u32,
        dynamic_rendering: true,
    };
    let gp = v
        .create_graphics_pipeline(gp_info)
        .expect("graphics pipeline");

    // ── Render target image ────────────────────────────────────────

    use ark_vk_binding::binding::ark::gpu::{
        buffer::{BufferCreateInfo, BufferUsage, Host as BufHost},
        image::{
            Extent3d, Host as ImageHost, ImageCreateFlags, ImageCreateInfo, ImageTiling, ImageType,
            ImageUsage, ImageViewCreateInfo, ImageViewType, SampleCount,
        },
        memory::{AllocateInfo, MemoryType},
    };

    let img_create = ImageCreateInfo {
        image_type: ImageType::Dim2d,
        format: vk::Format::R8G8B8A8_UNORM.as_raw() as u32,
        extent: Extent3d {
            width: W,
            height: H,
            depth: 1,
        },
        mip_levels: 1,
        array_layers: 1,
        samples: SampleCount::Sample1,
        tiling: ImageTiling::Optimal,
        usage: ImageUsage::COLOR_ATTACHMENT | ImageUsage::TRANSFER_SRC,
        create_flags: ImageCreateFlags::empty(),
    };
    let img_alloc = AllocateInfo {
        memory_type: MemoryType::PREFER_DEVICE,
    };
    let img = v.create_image(img_create, img_alloc).expect("create image");

    let view_create = ImageViewCreateInfo {
        image: Resource::new_borrow(img.rep()),
        view_type: ImageViewType::Dim2d,
        format: vk::Format::R8G8B8A8_UNORM.as_raw() as u32,
        subresource_range: ark_vk_binding::binding::ark::gpu::image::ImageSubresourceRange {
            aspect_mask: ark_vk_binding::binding::ark::gpu::image::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        },
        swizzle: None,
    };
    let view = v.create_image_view(view_create).expect("create image view");

    // ── Staging buffer ─────────────────────────────────────────────

    let img_bytes: u64 = (W * H * 4) as u64;
    let staging_create = BufferCreateInfo {
        size: img_bytes,
        usage: BufferUsage::TRANSFER_DST,
        sharing_mode: None,
    };
    let staging_alloc = AllocateInfo {
        memory_type: MemoryType::PREFER_HOST | MemoryType::HOST_RANDOM_ACCESS,
    };
    let staging = v
        .buffer_from_data(staging_create, staging_alloc, vec![0u8; img_bytes as usize])
        .expect("staging buffer");

    // ── Record command buffer ──────────────────────────────────────

    use ark_vk_binding::binding::ark::gpu::{
        command_buffer::{
            BufferImageCopy, CommandBufferUsage, Host as CmdHost, HostCommandBufferBuilder,
            ImageAspectFlags, ImageBarrier, ImageSubresourceLayers, ImageSubresourceRange,
            MemoryBarrier, Offset3d, Rect2d, RenderingColorAttachment,
            RenderingDepthStencilAttachment, Viewport,
        },
        core::QueueFamily,
        queue::Host as QueueHost,
    };

    let builder =
        v.primary_command_buffer(QueueFamily::Graphics, CommandBufferUsage::OneTimeSubmit);
    // b is no longer used; replaced by Resource::new_borrow at each call site

    // Pipeline barrier: undefined → color attachment optimal
    v.pipeline_barrier(
        Resource::new_borrow(builder.rep()),
        vec![MemoryBarrier {
            src_stage: vk::PipelineStageFlags::TOP_OF_PIPE.bits(),
            dst_stage: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT.bits(),
            global: false,
            buffer: None,
            image: Some(ImageBarrier {
                src_access: vk::AccessFlags::empty().bits(),
                dst_access: vk::AccessFlags::COLOR_ATTACHMENT_WRITE.bits(),
                old_layout: vk::ImageLayout::UNDEFINED.as_raw() as u32,
                new_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL.as_raw() as u32,
                image: Resource::new_borrow(img.rep()),
                subresource_range: ImageSubresourceRange {
                    aspect_mask: ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                },
            }),
        }],
    )
    .expect("barrier");

    // Begin rendering
    let render_area = Rect2d {
        offset_x: 0,
        offset_y: 0,
        width: W,
        height: H,
    };
    let color_attachment = RenderingColorAttachment {
        image_view: Resource::new_borrow(view.rep()),
        image_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL.as_raw() as u32,
        resolve_image_view: None,
        resolve_image_layout: 0,
        load_op: vk::AttachmentLoadOp::CLEAR.as_raw() as u32,
        store_op: vk::AttachmentStoreOp::STORE.as_raw() as u32,
        clear_value: Some(
            ark_vk_binding::binding::ark::gpu::command_buffer::ClearColor {
                floats: (0.1, 0.1, 0.15, 1.0),
            },
        ),
    };

    v.begin_rendering(
        Resource::new_borrow(builder.rep()),
        render_area,
        1,
        vec![color_attachment],
        None as Option<RenderingDepthStencilAttachment>,
    )
    .expect("begin rendering");

    // Bind graphics pipeline via WIT
    v.bind_graphics_pipeline(
        Resource::new_borrow(builder.rep()),
        Resource::new_borrow(gp.rep()),
    )
    .expect("bind graphics pipeline");

    let vp = Viewport {
        x: 0.0,
        y: 0.0,
        width: W as f32,
        height: H as f32,
        min_depth: 0.0,
        max_depth: 1.0,
    };
    v.set_viewport(Resource::new_borrow(builder.rep()), 0, vec![vp])
        .expect("set viewport");
    v.set_scissor(Resource::new_borrow(builder.rep()), 0, vec![render_area])
        .expect("set scissor");

    v.draw(Resource::new_borrow(builder.rep()), 3, 1, 0, 0)
        .expect("draw");

    v.end_rendering(Resource::new_borrow(builder.rep()))
        .expect("end rendering");

    // Pipeline barrier: color attachment → transfer src
    v.pipeline_barrier(
        Resource::new_borrow(builder.rep()),
        vec![MemoryBarrier {
            src_stage: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT.bits(),
            dst_stage: vk::PipelineStageFlags::TRANSFER.bits(),
            global: false,
            buffer: None,
            image: Some(ImageBarrier {
                src_access: vk::AccessFlags::COLOR_ATTACHMENT_WRITE.bits(),
                dst_access: vk::AccessFlags::TRANSFER_READ.bits(),
                old_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL.as_raw() as u32,
                new_layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL.as_raw() as u32,
                image: Resource::new_borrow(img.rep()),
                subresource_range: ImageSubresourceRange {
                    aspect_mask: ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                },
            }),
        }],
    )
    .expect("barrier");

    // Copy image to staging buffer
    let copy_region = BufferImageCopy {
        buffer_offset: 0,
        buffer_row_length: 0,
        buffer_image_height: 0,
        image_subresource: ImageSubresourceLayers {
            aspect_mask: ark_vk_binding::binding::ark::gpu::command_buffer::ImageAspectFlags::COLOR,
            mip_level: 0,
            base_array_layer: 0,
            layer_count: 1,
        },
        image_offset: Offset3d { x: 0, y: 0, z: 0 },
        image_extent: Extent3d {
            width: W,
            height: H,
            depth: 1,
        },
    };
    v.copy_image_to_buffer(
        Resource::new_borrow(builder.rep()),
        Resource::new_borrow(img.rep()),
        6, // VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL
        Resource::new_borrow(staging.rep()),
        vec![copy_region],
    )
    .expect("copy image to buffer");

    let cb = v.build_command_buffer(builder);

    // ── Submit ─────────────────────────────────────────────────────

    let q = v.graphics();
    v.submit(
        Resource::new_borrow(q.rep()),
        vec![Resource::new_borrow(cb.rep())],
        vec![],
        vec![],
        None,
    )
    .expect("submit");
    v.wait_idle(Resource::new_borrow(q.rep()))
        .expect("wait idle");

    // ── Read back and save PNG ─────────────────────────────────────

    let result = v.read(staging, 0, img_bytes).expect("read staging");

    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/target/graphics_triangle.png");
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let file = std::fs::File::create(path).expect("create PNG file");
    let mut encoder = png::Encoder::new(file, W, H);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("PNG header");
    writer.write_image_data(&result).expect("write PNG data");
    drop(writer);
    println!("✓ graphics_pipeline_triangle: written to {path}");
}

// ── GLSL compilation helper ───────────────────────────────────────────

fn compile_glsl(source: &str, entry: &str, kind: shaderc::ShaderKind) -> Vec<u32> {
    let compiler = shaderc::Compiler::new().expect("shaderc compiler");
    let mut options = shaderc::CompileOptions::new().expect("shaderc options");
    options.set_source_language(shaderc::SourceLanguage::GLSL);
    options.set_target_env(
        shaderc::TargetEnv::Vulkan,
        shaderc::EnvVersion::Vulkan1_3 as u32,
    );
    options.set_optimization_level(shaderc::OptimizationLevel::Zero);

    let result = compiler
        .compile_into_spirv(source, kind, "shader", entry, Some(&options))
        .expect("GLSL compile failed");
    result.as_binary().to_vec()
}
