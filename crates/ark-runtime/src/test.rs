//! Vulkan host-side integration tests.
//!
//! Uses `VkContextView` methods directly, with Vulkan validation layers
//! when available.
//!
//! ```text
//! cargo test -p ark-runtime -- --test-threads=1
//! ```

use std::{borrow::Cow, collections::HashMap, ffi::CStr};

use vulkanalia::{
    loader::{LibloadingLoader, LIBRARY},
    vk::{self, DeviceV1_0, EntryV1_0, HasBuilder, InstanceV1_0},
    Entry, Instance, ResultExt,
};
use vulkanalia_vma::vma::VmaAllocator;
use vulkanalia_vma::{Allocator, AllocatorOptions};
use wasmtime_wasi::ResourceTable;

use ark_vk_binding::{VkContextOwned, VkContextView};
use ark_vk_binding::binding::ark::gpu::{
    buffer::Host as BufHost,
    buffer::HostBuffer as BufResource,
    shader::Host as ShaderHost,
    descriptor::Host as DescHost,
    pipeline::Host as PipeHost,
    command_buffer::Host as CmdHost,
    command_buffer::HostCommandBufferBuilder as CmdBuilder,
    queue::Host as QueueHost,
    queue::HostQueue,
};

// ── Shader compilation helper ─────────────────────────────────────────

fn compile_compute(glsl: &str, entry: &str) -> Vec<u32> {
    let compiler = shaderc::Compiler::new().expect("shaderc compiler");
    let mut options = shaderc::CompileOptions::new().expect("shaderc options");
    options.set_source_language(shaderc::SourceLanguage::GLSL);
    options.set_target_env(shaderc::TargetEnv::Vulkan, shaderc::EnvVersion::Vulkan1_3 as u32);
    options.set_optimization_level(shaderc::OptimizationLevel::Zero);

    let result = compiler
        .compile_into_spirv(glsl, shaderc::ShaderKind::Compute, "shader.comp", entry, Some(&options))
        .expect("GLSL compile failed");
    let binary = result.as_binary().to_vec();
    let num_warnings = result.get_num_warnings();
    if num_warnings > 0 {
        println!("SPIR-V warnings: {}", result.get_warning_messages());
    }
    println!("SPIR-V compiled: {} words (magic={:08x}, first few: {:08x?})",
        binary.len(),
        binary.first().copied().unwrap_or(0),
        &binary[..binary.len().min(5)]);
    assert!(!binary.is_empty() && binary[0] == 0x07230203, "invalid SPIR-V magic number");
    binary
}

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
            vec![CStr::from_bytes_with_nul(b"VK_LAYER_KHRONOS_validation\0").unwrap()]
        } else {
            vec![]
        };
        let layer_ptrs: Vec<*const i8> = layers.iter().map(|l| l.as_ptr()).collect();

        let extensions = vec![vk::KHR_GET_PHYSICAL_DEVICE_PROPERTIES2_EXTENSION.name.as_ptr()];

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
        let mut vk13 = vk::PhysicalDeviceVulkan13Features::builder()
            .dynamic_rendering(true);

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
        let vma_wrapper = unsafe { vulkanalia_vma::Allocator::new(&alloc_opts) }.expect("VMA create");
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

        Self { _entry: entry, _instance: instance, _device: device, owned }
    }

    fn ctx_view<'a>(
        owned: &'a VkContextOwned,
        table: &'a mut ResourceTable,
        files: &'a HashMap<String, Cow<'static, [u8]>>,
    ) -> VkContextView<'a> {
        VkContextView { owned, table, files }
    }
}

fn has_validation_layer(entry: &Entry) -> bool {
    let Ok(props) = (unsafe { entry.enumerate_instance_layer_properties() }) else { return false };
    let target = CStr::from_bytes_with_nul(b"VK_LAYER_KHRONOS_validation\0").unwrap();
    props.iter().any(|lp| unsafe { CStr::from_ptr(lp.layer_name.as_ptr()) } == target)
}

fn pick_physical_device(
    instance: &Instance,
    devices: &[vk::PhysicalDevice],
) -> (vk::PhysicalDevice, u32, u32, u32) {
    let mut fallback = None;
    for &pd in devices {
        let props = unsafe { instance.get_physical_device_properties(pd) };
        let qf_props = unsafe { instance.get_physical_device_queue_family_properties(pd) };

        let graphics = qf_props.iter().position(|p| p.queue_flags.contains(vk::QueueFlags::GRAPHICS));
        let compute = qf_props.iter().position(|p| p.queue_flags.contains(vk::QueueFlags::COMPUTE));
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
    dsl: wasmtime::component::Resource<ark_vk_binding::binding::ark::gpu::descriptor::DescriptorSetLayout>,
    set: wasmtime::component::Resource<ark_vk_binding::binding::ark::gpu::descriptor::DescriptorSet>,
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

    let pool = v.create_descriptor_pool(
        1,
        vec![PoolSize { descriptor_type: DescriptorType::StorageBuffer, descriptor_count: 1 }],
        DescriptorPoolCreateFlags::empty(),
    ).expect("pool");
    // Borrow dsl for allocation and layout creation
    let dsl_alloc = Resource::new_borrow(dsl.rep());
    let set = v.allocate_descriptor_set(pool, dsl_alloc, vec![]).expect("allocate set");

    // Borrow dsl for pipeline layout
    let dsl_layout = Resource::new_borrow(dsl.rep());
    let pl = v.create_pipeline_layout(
        vec![DescriptorSetInfo { layout: dsl_layout, set: 0 }],
        vec![],
    ).expect("pipeline layout");

    // Borrow pl and shader for pipeline creation
    let pl_cp = Resource::new_borrow(pl.rep());
    let shader_cp = Resource::new_borrow(shader.rep());
    let cp = v.create_compute_pipeline(pl_cp, shader_cp, entry.into())
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
    let spirv = compile_compute(
        r"#version 460
        layout(local_size_x = 64) in;
        layout(set = 0, binding = 0) buffer Data { float v[]; };
        void main() {
            uint idx = gl_GlobalInvocationID.x;
            v[idx] = v[idx] + 1.0;
        }",
        "main",
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
    let alloc = AllocateInfo { memory_type: MemoryType::PREFER_DEVICE | MemoryType::HOST_SEQUENTIAL_WRITE };
    let data_bytes =
        unsafe { std::slice::from_raw_parts(data_in.as_ptr() as *const u8, (N as usize) * 4) }.to_vec();
    let buf = v.buffer_from_data(create_info, alloc, data_bytes).expect("create buffer");

    let cps = build_compute_pipeline(v, &spirv, "main");

    // Write descriptor
    use ark_vk_binding::binding::ark::gpu::descriptor::{
        BufferDescriptorInfo, DescriptorType, DescriptorWrite,
    };
    use wasmtime::component::Resource;
    v.write_descriptor_set(
        Resource::new_borrow(cps.set.rep()),
        vec![DescriptorWrite {
            binding: 0, dst_array_element: 0, descriptor_count: 1,
            descriptor_type: DescriptorType::StorageBuffer,
            buffer_info: Some(BufferDescriptorInfo { buffer: Resource::new_borrow(buf.rep()), offset: 0, range: N * 4 }),
            image_info: None,
        }],
    ).expect("write descriptor");

    use ark_vk_binding::binding::ark::gpu::{
        command_buffer::{CommandBufferUsage, PipelineBindPoint},
        core::QueueFamily,
    };

    // Record
    let builder = v.primary_command_buffer(QueueFamily::Compute, CommandBufferUsage::OneTimeSubmit);
    v.bind_compute_pipeline(Resource::new_borrow(builder.rep()), Resource::new_borrow(cps.cp.rep())).expect("bind cp");
    v.bind_descriptor_sets(
        Resource::new_borrow(builder.rep()), PipelineBindPoint::Compute,
        Resource::new_borrow(cps.pl.rep()), 0,
        vec![Resource::new_borrow(cps.set.rep())],
    ).expect("bind ds");
    v.dispatch(Resource::new_borrow(builder.rep()), (N as u32).div_ceil(64), 1, 1).expect("dispatch");

    let cb = v.build_command_buffer(builder);
    let q = v.compute();
    v.submit(Resource::new_borrow(q.rep()), vec![Resource::new_borrow(cb.rep())], vec![], vec![], None).expect("submit");
    v.wait_idle(Resource::new_borrow(q.rep())).expect("wait idle");

    // Verify
    let result_bytes = v.read(buf, 0, N * 4).expect("read buffer");
    let result_f32: &[f32] =
        unsafe { std::slice::from_raw_parts(result_bytes.as_ptr() as *const f32, N as usize) };

    for i in 0..N as usize {
        let expected = i as f32 + 1.0;
        assert!((result_f32[i] - expected).abs() < 0.005, "[{i}] expected {expected}, got {}", result_f32[i]);
    }
    println!("✓ compute_add_f32: {N} elements OK (first={:.1}, last={:.1})", result_f32[0], result_f32[N as usize - 1]);
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
    let spirv = compile_compute(
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
    let alloc = AllocateInfo { memory_type: MemoryType::PREFER_DEVICE | MemoryType::HOST_RANDOM_ACCESS };
    // buffer_from_data forces HOST_VISIBLE memory; buffer_zeroed does not.
    let out_buf = v.buffer_from_data(create_info, alloc, vec![0u8; img_size as usize])
        .expect("create output buffer");

    let cps = build_compute_pipeline(v, &spirv, "main");

    use ark_vk_binding::binding::ark::gpu::descriptor::{
        BufferDescriptorInfo, DescriptorType, DescriptorWrite,
    };
    use wasmtime::component::Resource;
    v.write_descriptor_set(
        Resource::new_borrow(cps.set.rep()),
        vec![DescriptorWrite {
            binding: 0, dst_array_element: 0, descriptor_count: 1,
            descriptor_type: DescriptorType::StorageBuffer,
            buffer_info: Some(BufferDescriptorInfo { buffer: Resource::new_borrow(out_buf.rep()), offset: 0, range: img_size }),
            image_info: None,
        }],
    ).expect("write descriptor");

    use ark_vk_binding::binding::ark::gpu::{
        command_buffer::{CommandBufferUsage, PipelineBindPoint},
        core::QueueFamily,
    };

    let builder = v.primary_command_buffer(QueueFamily::Compute, CommandBufferUsage::OneTimeSubmit);
    v.bind_compute_pipeline(Resource::new_borrow(builder.rep()), Resource::new_borrow(cps.cp.rep())).expect("bind cp");
    v.bind_descriptor_sets(
        Resource::new_borrow(builder.rep()), PipelineBindPoint::Compute,
        Resource::new_borrow(cps.pl.rep()), 0,
        vec![Resource::new_borrow(cps.set.rep())],
    ).expect("bind ds");
    v.dispatch(Resource::new_borrow(builder.rep()), W.div_ceil(8), H.div_ceil(8), 1).expect("dispatch");

    let cb = v.build_command_buffer(builder);
    let q = v.compute();
    v.submit(Resource::new_borrow(q.rep()), vec![Resource::new_borrow(cb.rep())], vec![], vec![], None).expect("submit");
    v.wait_idle(Resource::new_borrow(q.rep())).expect("wait idle");

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
