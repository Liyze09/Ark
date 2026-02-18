use std::sync::{Arc, Mutex};

use anyhow::{Ok, anyhow};
use dashmap::DashMap;
use vulkano::{buffer::{BufferUsage, Subbuffer, allocator::{SubbufferAllocator, SubbufferAllocatorCreateInfo}}, command_buffer::CommandBufferUsage, memory::allocator::{MemoryTypeFilter, StandardMemoryAllocator}, sync::{self, GpuFuture}};

use crate::backend::{QueueType, VkBackend, state::RenderState};

pub struct BuiltSections {
    map: DashMap<u64, Section>,
    subbuffer_allocator: Mutex<SubbufferAllocator>,
}

impl BuiltSections {
    pub fn new(memory_allocator: Arc<StandardMemoryAllocator>) -> Self {
        Self {
            map: DashMap::new(),
            subbuffer_allocator: Mutex::new(SubbufferAllocator::new(memory_allocator, 
                SubbufferAllocatorCreateInfo {
                    arena_size: 16* (1024^2),
                    buffer_usage: BufferUsage::TRANSFER_DST | BufferUsage::STORAGE_BUFFER | BufferUsage::TRANSFER_SRC,
                    memory_type_filter: MemoryTypeFilter::PREFER_DEVICE, 
                    ..Default::default()
                }
            ))
        }
    }

    pub fn upload(&self, backend: &VkBackend, headers: Vec<SectionHeader>, data: Vec<&[u8]>) -> anyhow::Result<()> {
        let mut command_buffer_builder = backend.allocate_command_buffer(
            QueueType::Transfer, CommandBufferUsage::OneTimeSubmit)?;
        for (header, data) in headers.into_iter().zip(data.into_iter()) {
            let buf = backend.transfer_data(data, &*self.subbuffer_allocator.lock().map_err(|err| anyhow!("terrain.rs:Error in locking subbuffer allocator: {}", err))?, &mut command_buffer_builder)?;
            self.map.insert(header.header, Section { header, data: buf });
        }
        let command_buffer = command_buffer_builder.build()?;
        backend.state_manager().acquire(RenderState::ChunkUploading)?;
        let fence = sync::now(backend.device().clone())
            .then_execute(backend.queue().transfer.clone(), command_buffer)?
            .then_signal_fence_and_flush()?;
        backend.state_manager().release(RenderState::ChunkUploading, Some(Box::new(fence)))?;
        Ok(())
    }

    pub fn remove(&self, key: u64) {
        self.map.remove(&key);
    }
}
pub struct Section {
    pub header: SectionHeader,
    pub data: Subbuffer<[u8]>,
}

pub struct SectionHeader {
    pub x: i32,
    pub y: u8,
    pub z: i32,
    pub block_count: u16,
    pub header: u64
}

impl SectionHeader {
    pub fn new(header: u64) -> Self {
        const X_MASK: u64   = 0x3F_FFFF;
        const Z_MASK: u64   = 0x3F_FFFF;
        const Y_MASK: u64   = 0xFF;
        const COUNT_MASK: u64 = 0xFFF;

        let x = (header & X_MASK) as i32;
        let z = ((header >> 22) & Z_MASK) as i32;
        let y = ((header >> 44) & Y_MASK) as u8;
        let block_count = ((header >> 52) & COUNT_MASK) as u16;

        Self { x, z, y, block_count, header }
    }
}
