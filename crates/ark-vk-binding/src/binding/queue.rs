use vulkanalia::vk::{self, DeviceV1_0, DeviceV1_3, HasBuilder, SuccessCode};
use wasmtime::component::Resource;

use crate::{
    VkContextView,
    binding::{
        sync::vk_pipeline_stage_flags,

        ark::gpu::{
            command_buffer::CommandBuffer as WitCommandBuffer,
            core::VulkanError,
            queue::{
                FenceFuture, Host, HostFenceFuture, HostQueue, HostSubmission, Queue,
                Submission,
            },
            sync::{
                Fence as WitFence, SemaphoreInfo, SemaphoreSubmitInfo,
            },
        },
        vk_err,
    },
};

// ── Internal types ──

#[allow(dead_code)]
struct GpuQueue {
    queue: vk::Queue,
    family_index: u32,
}

struct Batch {
    queue: vk::Queue,
    cmds: Vec<vk::CommandBuffer>,
    begins_with_barrier: bool,
}

struct GpuSubmission {
    batches: Vec<Batch>,
    force_new_batch: bool,
}

struct GpuFenceFuture {
    fence: vk::Fence,
    semaphores: Vec<vk::Semaphore>,
}

// ── Helpers ──

/// Resolve a `SemaphoreInfo` into a raw `vk::Semaphore` + optional timeline value.
fn resolve_sem_info(
    ctx: &mut VkContextView<'_>,
    info: &SemaphoreInfo,
) -> (vk::Semaphore, u64) {
    match info {
        SemaphoreInfo::Binary(sem) => {
            let key = Resource::<super::sync::GpuBinarySemaphore>::new_borrow(sem.rep());
            let gpu_sem = ctx.table.get(&key).expect("binary semaphore not found");
            (gpu_sem.semaphore, 0)
        }
        SemaphoreInfo::Timeline(tuple) => {
            let key = Resource::<super::sync::GpuTimelineSemaphore>::new_borrow(tuple.0.rep());
            let gpu_sem = ctx.table.get(&key).expect("timeline semaphore not found");
            (gpu_sem.semaphore, tuple.1)
        }
    }
}

// ── Host-level queue accessors ──

impl Host for VkContextView<'_> {
    fn graphics(&mut self) -> Resource<Queue> {
        let q = GpuQueue {
            queue: self.graphics_queue,
            family_index: self.graphics_queue_family_index,
        };
        let handle = self.table.push(q).expect("ResourceTable push failed");
        Resource::new_own(handle.rep())
    }

    fn compute(&mut self) -> Resource<Queue> {
        let q = GpuQueue {
            queue: self.compute_queue,
            family_index: self.compute_queue_family_index,
        };
        let handle = self.table.push(q).expect("ResourceTable push failed");
        Resource::new_own(handle.rep())
    }

    fn transfer(&mut self) -> Resource<Queue> {
        let q = GpuQueue {
            queue: self.transfer_queue,
            family_index: self.transfer_queue_family_index,
        };
        let handle = self.table.push(q).expect("ResourceTable push failed");
        Resource::new_own(handle.rep())
    }
}

// ── Queue methods ──

impl HostQueue for VkContextView<'_> {
    fn submit(
        &mut self,
        self_: Resource<Queue>,
        command_buffers: Vec<Resource<WitCommandBuffer>>,
        waits: Vec<SemaphoreSubmitInfo>,
        signals: Vec<SemaphoreSubmitInfo>,
        fence: Option<Resource<WitFence>>,
    ) -> Result<(), VulkanError> {
        let queue_vk = {
            let queue_key = Resource::<GpuQueue>::new_borrow(self_.rep());
            let gpu_queue = self.table.get(&queue_key).map_err(|_| VulkanError::Unknown)?;
            gpu_queue.queue
        };

        // Collect command buffers
        let mut cmd_bufs_vk: Vec<vk::CommandBuffer> = Vec::new();
        let mut cmd_keys: Vec<Resource<WitCommandBuffer>> = Vec::new();
        for cb in &command_buffers {
            let cb_key = Resource::<super::command::GpuCommandBuffer>::new_borrow(cb.rep());
            let gpu_cb = self.table.get(&cb_key).map_err(|_| VulkanError::Unknown)?;
            cmd_bufs_vk.push(gpu_cb.cmd);
            cmd_keys.push(Resource::<WitCommandBuffer>::new_borrow(cb.rep()));
        }

        // Build VkSemaphoreSubmitInfo entries for waits
        let mut wait_infos_vk: Vec<vk::SemaphoreSubmitInfo> = Vec::with_capacity(waits.len());
        for ssi in &waits {
            let (handle, value) = resolve_sem_info(self, &ssi.semaphore);
            let stage = vk_pipeline_stage_flags(ssi.stages);
            wait_infos_vk.push(
                vk::SemaphoreSubmitInfo::builder()
                    .semaphore(handle)
                    .value(value)
                    .stage_mask(stage)
                    .build(),
            );
        }

        // Build VkSemaphoreSubmitInfo entries for signals
        let mut signal_infos_vk: Vec<vk::SemaphoreSubmitInfo> = Vec::with_capacity(signals.len());
        for ssi in &signals {
            let (handle, value) = resolve_sem_info(self, &ssi.semaphore);
            let stage = vk_pipeline_stage_flags(ssi.stages);
            signal_infos_vk.push(
                vk::SemaphoreSubmitInfo::builder()
                    .semaphore(handle)
                    .value(value)
                    .stage_mask(stage)
                    .build(),
            );
        }

        // Build VkCommandBufferSubmitInfo
        let cmd_infos: Vec<vk::CommandBufferSubmitInfo> = cmd_bufs_vk
            .iter()
            .map(|&cmd| {
                vk::CommandBufferSubmitInfo::builder()
                    .command_buffer(cmd)
                    .build()
            })
            .collect();

        // Optional fence
        let fence_vk = if let Some(ref f) = fence {
            let fence_key = Resource::<super::sync::GpuFence>::new_borrow(f.rep());
            let f = self
                .table
                .get(&fence_key)
                .map_err(|_| VulkanError::Unknown)?;
            f.fence
        } else {
            vk::Fence::default()
        };

        let submit_info = vk::SubmitInfo2::builder()
            .wait_semaphore_infos(&wait_infos_vk)
            .command_buffer_infos(&cmd_infos)
            .signal_semaphore_infos(&signal_infos_vk);

        unsafe {
            self.vk_device()
                .queue_submit2(queue_vk, &[submit_info.build()], fence_vk)
        }
        .map_err(vk_err)?;

        let _ = cmd_keys;
        Ok(())
    }

    fn wait_idle(&mut self, self_: Resource<Queue>) -> Result<(), VulkanError> {
        let queue_key = Resource::<GpuQueue>::new_borrow(self_.rep());
        let gpu_queue = self
            .table
            .get(&queue_key)
            .map_err(|_| VulkanError::Unknown)?;
        unsafe { self.vk_device().queue_wait_idle(gpu_queue.queue) }.map_err(vk_err)
    }

    fn drop(&mut self, rep: Resource<Queue>) -> wasmtime::anyhow::Result<()> {
        let key = Resource::<GpuQueue>::new_own(rep.rep());
        self.table.delete(key)?;
        Ok(())
    }
}

// ── Submission helpers ──

unsafe fn submit_batch(
    vk_dev: &(impl std::borrow::Borrow<vk::DeviceCommands>, vk::Device),
    batch: &Batch,
    wait_sems: &[vk::Semaphore],
    wait_stages: &[vk::PipelineStageFlags],
    signal_sems: &[vk::Semaphore],
    fence: vk::Fence,
) -> Result<(), VulkanError> {
    if batch.cmds.is_empty() {
        return Ok(());
    }
    let submit_info = vk::SubmitInfo::builder()
        .wait_semaphores(wait_sems)
        .wait_dst_stage_mask(wait_stages)
        .command_buffers(&batch.cmds)
        .signal_semaphores(signal_sems);
    unsafe { vk_dev.queue_submit(batch.queue, &[submit_info.build()], fence) }.map_err(vk_err)
}

unsafe fn create_binary_semaphore(
    vk_dev: &(impl std::borrow::Borrow<vk::DeviceCommands>, vk::Device),
) -> Result<vk::Semaphore, VulkanError> {
    let info = vk::SemaphoreCreateInfo::builder();
    unsafe { vk_dev.create_semaphore(&info, None) }.map_err(vk_err)
}

// ── Submission builder ──

impl HostSubmission for VkContextView<'_> {
    fn new(&mut self) -> Resource<Submission> {
        let sub = GpuSubmission {
            batches: Vec::new(),
            force_new_batch: false,
        };
        let handle = self.table.push(sub).expect("ResourceTable push failed");
        Resource::new_own(handle.rep())
    }

    fn execute(
        &mut self,
        self_: Resource<Submission>,
        queue: Resource<Queue>,
        command_buffer: Resource<WitCommandBuffer>,
    ) {
        let queue_vk = {
            let queue_key = Resource::<GpuQueue>::new_borrow(queue.rep());
            let gpu_queue = self.table.get(&queue_key).expect("queue not found");
            gpu_queue.queue
        };

        let cb_key = Resource::<super::command::GpuCommandBuffer>::new_own(command_buffer.rep());
        let gpu_cb = self.table.delete(cb_key).expect("command buffer not found");

        let sub_key = Resource::<GpuSubmission>::new_borrow(self_.rep());
        let sub = self.table.get_mut(&sub_key).expect("submission not found");

        let force_new = sub.force_new_batch;
        let same_queue = sub
            .batches
            .last()
            .is_some_and(|last| last.queue == queue_vk);

        if force_new || !same_queue {
            sub.batches.push(Batch {
                queue: queue_vk,
                cmds: vec![gpu_cb.cmd],
                begins_with_barrier: force_new,
            });
            sub.force_new_batch = false;
        } else {
            sub.batches.last_mut().unwrap().cmds.push(gpu_cb.cmd);
        }
    }

    fn signal_semaphore(&mut self, self_: Resource<Submission>) {
        let sub_key = Resource::<GpuSubmission>::new_borrow(self_.rep());
        let sub = self.table.get_mut(&sub_key).expect("submission not found");
        sub.force_new_batch = true;
    }

    fn flush(&mut self, self_: Resource<Submission>) -> Result<(), VulkanError> {
        let sub_key = Resource::<GpuSubmission>::new_own(self_.rep());
        let sub = self.table.delete(sub_key).map_err(|_| VulkanError::Unknown)?;
        flush_impl(self, sub, vk::Fence::default(), false).map(|_| ())
    }

    fn signal_fence_and_flush(
        &mut self,
        self_: Resource<Submission>,
    ) -> Result<Resource<FenceFuture>, VulkanError> {
        let sub_key = Resource::<GpuSubmission>::new_own(self_.rep());
        let sub = self.table.delete(sub_key).map_err(|_| VulkanError::Unknown)?;

        let fence_info = vk::FenceCreateInfo::builder();
        let fence = unsafe { self.vk_device().create_fence(&fence_info, None) }.map_err(vk_err)?;

        if sub.batches.is_empty() {
            unsafe { self.vk_device().destroy_fence(fence, None) };
            let signaled_info =
                vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED);
            let fence =
                unsafe { self.vk_device().create_fence(&signaled_info, None) }.map_err(vk_err)?;
            return finish_fence_future(self, fence, Vec::new());
        }

        let semaphores = flush_impl(self, sub, fence, true)?;
        finish_fence_future(self, fence, semaphores)
    }

    fn drop(&mut self, rep: Resource<Submission>) -> wasmtime::anyhow::Result<()> {
        let key = Resource::<GpuSubmission>::new_own(rep.rep());
        self.table.delete(key)?;
        Ok(())
    }
}

fn flush_impl(
    ctx: &mut VkContextView<'_>,
    sub: GpuSubmission,
    final_fence: vk::Fence,
    return_semaphores: bool,
) -> Result<Vec<vk::Semaphore>, VulkanError> {
    let batches: Vec<&Batch> = sub.batches.iter().filter(|b| !b.cmds.is_empty()).collect();
    if batches.is_empty() {
        return Ok(Vec::new());
    }

    let barrier_count = batches.iter().filter(|b| b.begins_with_barrier).count();

    if barrier_count == 0 {
        for (i, batch) in batches.iter().enumerate() {
            let fence = if i == batches.len() - 1 { final_fence } else { vk::Fence::default() };
            let submit_info = vk::SubmitInfo::builder().command_buffers(&batch.cmds);
            unsafe {
                ctx.vk_device().queue_submit(batch.queue, &[submit_info.build()], fence)
            }.map_err(vk_err)?;
        }
        return Ok(Vec::new());
    }

    let vk_dev = ctx.vk_device();
    let semaphores: Vec<vk::Semaphore> = (0..barrier_count)
        .map(|_| unsafe { create_binary_semaphore(&vk_dev) })
        .collect::<Result<_, _>>()?;

    let mut next_sem: usize = 0;
    for (i, batch) in batches.iter().enumerate() {
        let (wait_slice, stage) = if batch.begins_with_barrier {
            let s = &semaphores[next_sem..next_sem + 1];
            next_sem += 1;
            (s, vec![vk::PipelineStageFlags::ALL_COMMANDS])
        } else {
            (&[][..], vec![])
        };

        let next_barrier = batches[i + 1..].iter().any(|b| b.begins_with_barrier);
        let signal_slice: &[vk::Semaphore] = if next_barrier {
            &semaphores[next_sem..next_sem + 1]
        } else {
            &[][..]
        };

        let fence = if i == batches.len() - 1 { final_fence } else { vk::Fence::default() };
        unsafe { submit_batch(&vk_dev, batch, wait_slice, &stage, signal_slice, fence) }?;
    }

    if return_semaphores {
        Ok(semaphores)
    } else {
        ctx.owned.pending_semaphores.lock().unwrap().extend(semaphores);
        Ok(Vec::new())
    }
}

fn finish_fence_future(
    ctx: &mut VkContextView<'_>,
    fence: vk::Fence,
    semaphores: Vec<vk::Semaphore>,
) -> Result<Resource<FenceFuture>, VulkanError> {
    let handle = ctx.table
        .push(GpuFenceFuture { fence, semaphores })
        .map_err(|_| VulkanError::OutOfHostMemory)?;
    Ok(Resource::new_own(handle.rep()))
}

// ── FenceFuture ──

impl HostFenceFuture for VkContextView<'_> {
    fn wait(&mut self, self_: Resource<FenceFuture>, timeout_ns: u64) -> Result<(), VulkanError> {
        let key = Resource::<GpuFenceFuture>::new_borrow(self_.rep());
        let ff = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        unsafe {
            self.vk_device()
                .wait_for_fences(&[ff.fence], true, timeout_ns)
                .map(|_| ())
                .map_err(vk_err)
        }
    }

    fn try_wait(&mut self, self_: Resource<FenceFuture>) -> Result<bool, VulkanError> {
        let key = Resource::<GpuFenceFuture>::new_borrow(self_.rep());
        let ff = self.table.get(&key).map_err(|_| VulkanError::Unknown)?;
        unsafe { 
            Ok(
                self.vk_device()
                    .get_fence_status(ff.fence)
                    .map_err(vk_err)?
                    .eq(&SuccessCode::SUCCESS)
            )
        }
    }

    fn drop(&mut self, rep: Resource<FenceFuture>) -> wasmtime::anyhow::Result<()> {
        let key = Resource::<GpuFenceFuture>::new_own(rep.rep());
        let ff = self.table.delete(key)?;
        unsafe {
            self.vk_device().destroy_fence(ff.fence, None);
            for sem in &ff.semaphores {
                self.vk_device().destroy_semaphore(*sem, None);
            }
        }
        Ok(())
    }
}
