use vulkanalia::prelude::v1_0::*;
use vulkanalia::vk::{self, HasBuilder};
use wasmtime::component::Resource;

use crate::{
    binding::{
        ark::gpu::{
            core::VulkanError,
            shader::{Host, HostShaderModule, ShaderModule},
        },
        vk_err,
    },
    VkContextView,
};


impl Host for VkContextView<'_> {}

pub(crate) struct GpuShaderModule {
    pub(crate) module: vk::ShaderModule,
}

impl HostShaderModule for VkContextView<'_> {
    fn shader_from_bytes(&mut self, code: Vec<u32>) -> Result<Resource<ShaderModule>, VulkanError> {
        let info = vk::ShaderModuleCreateInfo {
            s_type: vk::StructureType::SHADER_MODULE_CREATE_INFO,
            next: std::ptr::null(),
            flags: vk::ShaderModuleCreateFlags::empty(),
            code_size: code.len() * 4,
            code: code.as_ptr(),
        };
        let module =
            unsafe { self.vk_device().create_shader_module(&info, None) }.map_err(vk_err)?;
        let handle = self
            .table
            .push(GpuShaderModule { module })
            .map_err(|_| VulkanError::OutOfHostMemory)?;
        Ok(Resource::new_own(handle.rep()))
    }

    fn shader_from_package(&mut self, path: String) -> Result<Resource<ShaderModule>, VulkanError> {
        let Some(spirv_bytes) = self.files.get(&path) else {
            return Err(VulkanError::Unnamed(format!("shader not found in package: {}", path)));
        };

        if spirv_bytes.len() % 4 != 0 {
            return Err(VulkanError::InvalidShader);
        }

        let code: Vec<u32> = spirv_bytes
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();

        let info = vk::ShaderModuleCreateInfo {
            s_type: vk::StructureType::SHADER_MODULE_CREATE_INFO,
            next: std::ptr::null(),
            flags: vk::ShaderModuleCreateFlags::empty(),
            code_size: code.len() * 4,
            code: code.as_ptr(),
        };
        let module =
            unsafe { self.vk_device().create_shader_module(&info, None) }.map_err(vk_err)?;
        let handle = self
            .table
            .push(GpuShaderModule { module })
            .map_err(|_| VulkanError::OutOfHostMemory)?;
        Ok(Resource::new_own(handle.rep()))
    }

    fn drop(&mut self, rep: Resource<ShaderModule>) -> wasmtime::anyhow::Result<()> {
        let key = Resource::<GpuShaderModule>::new_own(rep.rep());
        let shader = self.table.delete(key)?;
        unsafe {
            self.vk_device().destroy_shader_module(shader.module, None);
        }
        Ok(())
    }
}
