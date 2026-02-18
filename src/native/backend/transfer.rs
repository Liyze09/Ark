use std::num::NonZeroU64;

use anyhow::{Result, anyhow};
use vulkano::{buffer::{BufferContents, Subbuffer, allocator::SubbufferAllocator}, command_buffer::{AutoCommandBufferBuilder, CopyBufferInfo}};

use crate::backend::VkBackend;

pub trait SubbufferProvider<T: BufferContents> {
    fn subbuffer(&self, size: NonZeroU64) -> Result<Subbuffer<[T]>>;
}

impl<T: BufferContents> SubbufferProvider<T> for Subbuffer<[T]> {
    fn subbuffer(&self, size: NonZeroU64) -> Result<Subbuffer<[T]>> {
        if self.len() < size.into() {
            return Err(anyhow!("subbuffer size is too small"));
        }
        Ok(self.clone().slice(..u64::from(size)))
    }
}

impl<T: BufferContents> SubbufferProvider<T> for &SubbufferAllocator {
    fn subbuffer(&self, size: NonZeroU64) -> Result<Subbuffer<[T]>> {
        Ok(self.allocate_slice(size.into())?)
    }
}

impl VkBackend {
    pub fn transfer_data<L, D: BufferContents + Copy>(
        &self,
        data: &[D],
        subbuffer_provider: impl SubbufferProvider<D>,
        command_buffer: &mut AutoCommandBufferBuilder<L>,
    ) -> Result<Subbuffer<[D]>> {
        let target = subbuffer_provider.subbuffer(
            NonZeroU64::new(data.len() as u64)
            .ok_or(anyhow!("data length cannot be zero"))?
        )?;
        let host_subbuffer_alloc = self.host_subbuffer_alloc.lock().map_err(|err| {
            anyhow!(
                "backend.rs:error in locking host subbuffer allocator: {:?}",
                err
            )
        })?;
        let host_buffer = host_subbuffer_alloc.allocate_slice(data.len() as vulkano::DeviceSize)?;
        {
            let mut host_content = host_buffer.write()?;
            host_content.copy_from_slice(data);
        }
        command_buffer.copy_buffer(CopyBufferInfo::buffers(host_buffer.clone(), target.clone()))?;
        Ok(target)
    }
}
