pub mod buffer;
pub mod command;
pub mod descriptor;
pub mod image;
pub mod memory;
pub mod pipeline;
pub mod queue;
pub mod shader;
pub mod sync;

use vulkanalia::vk;
use wasmtime::component::bindgen;

use crate::binding::ark::gpu::core::VulkanError;

bindgen!({
    world: "vulkan",
    anyhow: true,
});

pub(crate) fn vk_err(err: vk::ErrorCode) -> VulkanError {
    match err.as_raw() {
        1 => VulkanError::NotReady,
        2 => VulkanError::Timeout,
        -1 => VulkanError::OutOfHostMemory,
        -2 => VulkanError::OutOfDeviceMemory,
        -3 => VulkanError::InitializationFailed,
        -4 => VulkanError::DeviceLost,
        -5 => VulkanError::MemoryMapFailed,
        -6 => VulkanError::LayerNotPresent,
        -7 => VulkanError::ExtensionNotPresent,
        -8 => VulkanError::FeatureNotPresent,
        -9 => VulkanError::IncompatibleDriver,
        -10 => VulkanError::TooManyObjects,
        -11 => VulkanError::FormatNotSupported,
        -12 => VulkanError::FragmentedPool,
        -1000069000 => VulkanError::OutOfPoolMemory,
        -1000072003 => VulkanError::InvalidExternalHandle,
        -1000161000 => VulkanError::Fragmentation,
        -1000257000 => VulkanError::InvalidOpaqueCaptureAddress,
        -1000003001 => VulkanError::IncompatibleDisplay,
        -1000174001 => VulkanError::NotPermitted,
        -1000000000 => VulkanError::SurfaceLost,
        -1000000001 => VulkanError::NativeWindowInUse,
        -1000001004 => VulkanError::OutOfDate,
        -1000023000 => VulkanError::ImageUsageNotSupported,
        -1000023001 => VulkanError::VideoPictureLayoutNotSupported,
        -1000023002 => VulkanError::VideoProfileOperationNotSupported,
        -1000023003 => VulkanError::VideoProfileFormatNotSupported,
        -1000023004 => VulkanError::VideoProfileCodecNotSupported,
        -1000023005 => VulkanError::VideoStdVersionNotSupported,
        -1000011001 => VulkanError::ValidationFailed,
        -1000000003 => VulkanError::FullScreenExclusiveModeLost,
        -1000158000 => VulkanError::InvalidDrmFormatModifierPlaneLayout,
        -1000012000 => VulkanError::InvalidShader,
        -1000299000 => VulkanError::InvalidVideoStdParameters,
        -1000338000 => VulkanError::CompressionExhausted,
        -13 => VulkanError::Unknown,
        other => VulkanError::Unnamed(format!("VK_ERROR_{}", other)),
    }
}
