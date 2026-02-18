pub mod backend;
pub mod shader;
pub mod texture;
pub mod terrain;

use crate::backend::VkBackend;
use crate::terrain::SectionHeader;
use ash::vk::HANDLE;
use jni::objects::{JClass, JFloatArray, JIntArray, JObjectArray, JString, JLongArray, ReleaseMode};
use jni::sys::{jint, jlong};
use jni::JNIEnv;
use mimalloc::MiMalloc;
use std::collections::HashMap;
use std::sync::Arc;
use vulkano::image::ImageUsage;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_liyze09_nexus_NexusClientMain_initNative<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> jlong {
    let context = match VkBackend::new() {
        Ok(context) => context,
        Err(err) => {
            env.throw_new(
                String::from("io/github/liyze09/nexus/exception/VulkanException"),
                err.to_string(),
            )
            .unwrap();
            return -1;
        }
    };
    Box::into_raw(Box::from(Arc::new(context))) as usize as u64 as i64
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_liyze09_nexus_NexusClientMain_getTextureSize<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ctx: jlong,
) -> jlong {
    let renderer = unsafe { load_context(ctx) };
    match renderer.target().get_value() {
        Ok(target) => target.size as i64 as jlong,
        Err(err) => {
            env.throw_new(
                String::from("io/github/liyze09/nexus/exception/VulkanException"),
                err.to_string(),
            )
            .unwrap();
            -1
        }
    }
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_liyze09_nexus_NexusClientMain_getGLReady<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    ctx: jlong,
) -> jlong {
    let renderer = unsafe { load_context(ctx) };
    renderer.semaphore().handle_gl_ready as i64 as jlong
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_liyze09_nexus_NexusClientMain_getGLComplete<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    ctx: jlong,
) -> jlong {
    let renderer = unsafe { load_context(ctx) };
    renderer.semaphore().handle_gl_complete as i64 as jlong
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_liyze09_nexus_NexusClientMain_close<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    ctx: jlong,
) {
    unsafe {
        drop(Box::from_raw(ctx as *mut VkBackend));
    }
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_liyze09_nexus_NexusClientMain_resize<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ctx: jlong,
    width: jint,
    height: jint,
) {
    let renderer = unsafe { load_context(ctx) };
    renderer.resize((width as u32, height as u32));
    match renderer.update() {
        Ok(_) => {}
        Err(err) => {
            env.throw_new(
                String::from("io/github/liyze09/nexus/exception/VulkanException"),
                err.to_string(),
            )
            .unwrap();
        }
    }
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_liyze09_nexus_NexusClientMain_render<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ctx: jlong,
) {
    let renderer = unsafe { load_context(ctx) };
    match renderer.render() {
        Ok(_) => {}
        Err(err) => {
            env.throw_new(
                String::from("io/github/liyze09/nexus/exception/VulkanException"),
                err.to_string(),
            )
            .unwrap();
        }
    }
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_liyze09_nexus_NexusClientMain_getGLTexture<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ctx: jlong,
) -> jlong {
    let renderer = unsafe { load_context(ctx) };
    match renderer.target().get_value() {
        Ok(target) => target.handle as i64 as jlong,
        Err(err) => {
            env.throw_new(
                String::from("io/github/liyze09/nexus/exception/VulkanException"),
                err.to_string(),
            )
            .unwrap();
            -1
        }
    }
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_liyze09_nexus_NexusClientMain_acquireVulkanTexture<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ctx: jlong,
    width: jint,
    height: jint,
    mip_levels: jint,
) -> jlong {
    let renderer = unsafe { load_context(ctx) };
    match renderer.create_external_texture(
        ImageUsage::SAMPLED
            | ImageUsage::TRANSFER_DST
            | ImageUsage::TRANSFER_SRC
            | ImageUsage::COLOR_ATTACHMENT
            | ImageUsage::STORAGE
            | ImageUsage::INPUT_ATTACHMENT,
        (width as u32, height as u32),
        mip_levels as u32,
    ) {
        Ok(target) => target.3 as i64 as jlong,
        Err(err) => {
            env.throw_new(
                String::from("io/github/liyze09/nexus/exception/VulkanException"),
                err.to_string(),
            )
            .unwrap();
            -1
        }
    }
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_liyze09_nexus_NexusClientMain_getVulkanTextureSize<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ctx: jlong,
    handle: jlong,
) -> jlong {
    let renderer = unsafe { load_context(ctx) };
    match renderer.get_texture_size_by_handle(handle as HANDLE) {
        Ok(size) => size as i64 as jlong,
        Err(err) => {
            env.throw_new(
                String::from("io/github/liyze09/nexus/exception/VulkanException"),
                err.to_string(),
            )
            .unwrap();
            -1
        }
    }
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_liyze09_nexus_NexusClientMain_uploadSections<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ctx: jlong,
    section_headers: JLongArray<'local>,
    section_data_addr: JLongArray<'local>,
) {
    match upload_sections(&mut env, ctx, section_headers, section_data_addr) {
        Ok(_) => {}
        Err(err) => {
            env.throw_new(
                String::from("io/github/liyze09/nexus/exception/VulkanException"),
                err.to_string(),
            )
            .unwrap();
        }
    }
}

fn upload_sections<'local>(
    env: &mut JNIEnv<'local>,
    ctx: jlong,
    section_headers: JLongArray<'local>,
    section_data_addr: JLongArray<'local>
) -> anyhow::Result<()> {
    let renderer = unsafe { load_context(ctx) };
    let mut section_data: Vec<&[u8]>  = Vec::with_capacity(env.get_array_length(&section_data_addr)? as usize);
    let section_headers_vec = unsafe { env.get_array_elements(&section_headers, ReleaseMode::NoCopyBack)? }
        .iter()
        .map(|&h| SectionHeader::new(h as u64)).collect::<Vec<_>>();
    let section_data_addr_vec = unsafe { env.get_array_elements(&section_data_addr, ReleaseMode::NoCopyBack)? };
    for i in 0..env.get_array_length(&section_data_addr)? {
        let data_addr = section_data_addr_vec[i as usize] as *const u8;
        let data_len = (section_headers_vec[i as usize].block_count * 5) as usize;
        let slice = unsafe { core::slice::from_raw_parts(data_addr, data_len) };
        section_data.push(slice);
    }
    renderer.terrain_manager().upload(&renderer, section_headers_vec, section_data)?;
    Ok(())
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_liyze09_nexus_NexusClientMain_syncAtlas<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ctx: jlong,
    texture_handle: jlong,
    atlas_name: JString<'local>,
    sprite_names: JObjectArray<'local>,
    sprite_x: JIntArray<'local>,
    sprite_y: JIntArray<'local>,
    sprite_width: JIntArray<'local>,
    sprite_height: JIntArray<'local>,
    sprite_u0: JFloatArray<'local>,
    sprite_v0: JFloatArray<'local>,
    sprite_u1: JFloatArray<'local>,
    sprite_v1: JFloatArray<'local>,
) {
    match sync_atlas(&mut env, ctx, texture_handle, atlas_name, sprite_names, sprite_x, sprite_y, sprite_width, sprite_height, sprite_u0, sprite_v0, sprite_u1, sprite_v1) {
        Ok(_) => {}
        Err(err) => {
            env.throw_new(
                String::from("io/github/liyze09/nexus/exception/VulkanException"),
                err.to_string(),
            ).unwrap();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_atlas<'local>(
    env: &mut JNIEnv<'local>,
    ctx: jlong,
    texture_handle: jlong,
    atlas_name: JString<'local>,
    sprite_names: JObjectArray<'local>,
    sprite_x: JIntArray<'local>,
    sprite_y: JIntArray<'local>,
    sprite_width: JIntArray<'local>,
    sprite_height: JIntArray<'local>,
    sprite_u0: JFloatArray<'local>,
    sprite_v0: JFloatArray<'local>,
    sprite_u1: JFloatArray<'local>,
    sprite_v1: JFloatArray<'local>
) -> anyhow::Result<()> {
    let renderer = unsafe { load_context(ctx) };

    let atlas_name_str = env.get_string(&atlas_name)?;
    let len = env.get_array_length(&sprite_names)?;

    // Get sprite names (object array)
    let mut sprite_names_vec: Vec<String> = Vec::with_capacity(len as usize);
    for i in 0..len {
        let jstr_obj = env.get_object_array_element(&sprite_names, i)?;
        let jstr = &jstr_obj.into();
        let rust_str = env.get_string(jstr)?;
        sprite_names_vec.push(rust_str.into());
    }

    let get_int_array = |env: &mut JNIEnv, array: &JIntArray, name: &str| -> anyhow::Result<Vec<i32>> {
        match unsafe { env.get_array_elements(array, ReleaseMode::NoCopyBack) } {
            Ok(elements) => {
                let vec: Vec<i32> = elements.iter().copied().collect();
                Ok(vec)
            }
            Err(err) => {
                Err(anyhow::anyhow!("Failed to get {} array elements: {}", name, err))
            }
        }
    };

    let get_float_array = |env: &mut JNIEnv, array: &JFloatArray, name: &str| -> anyhow::Result<Vec<f32>> {
        // Use ReleaseMode::NoCopyBack since we only read the data
        match unsafe { env.get_array_elements_critical(array, ReleaseMode::NoCopyBack) } {
            Ok(elements) => {
                let vec: Vec<f32> = elements.iter().copied().collect();
                Ok(vec)
            }
            Err(err) => {
                Err(anyhow::anyhow!("Failed to get {} array elements: {}", name, err))
            }
        }
    };

    let sprite_x_vec = get_int_array(env, &sprite_x, "sprite_x")?;
    let sprite_y_vec = get_int_array(env, &sprite_y, "sprite_y")?;
    let sprite_width_vec = get_int_array(env, &sprite_width, "sprite_width")?;
    let sprite_height_vec = get_int_array(env, &sprite_height, "sprite_height")?;
    let sprite_u0_vec = get_float_array(env, &sprite_u0, "sprite_u0")?;
    let sprite_v0_vec = get_float_array(env, &sprite_v0, "sprite_v0")?;
    let sprite_u1_vec = get_float_array(env, &sprite_u1, "sprite_u1")?;
    let sprite_v1_vec = get_float_array(env, &sprite_v1, "sprite_v1")?;

    // Build sprite map
    let mut sprite_map = HashMap::new();
    for i in 0..len as usize {
        let sprite_info = crate::texture::SpriteInfo {
            name: sprite_names_vec[i].clone(),
            x: sprite_x_vec[i] as u32,
            y: sprite_y_vec[i] as u32,
            width: sprite_width_vec[i] as u32,
            height: sprite_height_vec[i] as u32,
            u0: sprite_u0_vec[i],
            v0: sprite_v0_vec[i],
            u1: sprite_u1_vec[i],
            v1: sprite_v1_vec[i],
        };
        sprite_map.insert(sprite_names_vec[i].clone(), sprite_info);
    }

    renderer.sync_atlas(
        renderer.get_exported_image_by_handle(texture_handle as HANDLE)?.image,
        atlas_name_str.into(),
        sprite_map,
    );
    Ok(())
}

unsafe fn load_context(addr: i64) -> Arc<VkBackend> {
    unsafe {
        let ptr = addr as *mut Arc<VkBackend>;
        (*ptr).clone()
    }
}
