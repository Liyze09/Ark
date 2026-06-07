use vulkanalia::vk::{self, DeviceV1_0, HasBuilder};
use wasmtime::component::Resource;

use crate::{
    binding::{
        ark::gpu::{
            command_buffer::CommandBuffer as WitCommandBuffer,
            core::VulkanError,
            queue::{
                FenceFuture, Host, HostFenceFuture, HostQueue, HostSubmission, Queue,
                SemaphoreWaitInfo, Submission,
            },
            sync::{Fence as WitFence, Semaphore as WitSemaphore},
        },
        vk_err,
    },
    VkContextView,
};

// ── Internal types ──

/// A Vulkan queue paired with its family index, mirroring Java's `VulkanQueue`.
#[allow(dead_code)]
struct GpuQueue {
    queue: vk::Queue,
    family_index: u32,
}

/// A group of command buffers submitted together on the same queue.
struct Batch {
    queue: vk::Queue,
    cmds: Vec<vk::CommandBuffer>,
    /// True if this batch was started immediately after a `signal_semaphore()`
    /// call and must be synchronised with the previous batch via a binary
    /// semaphore at flush time.
    begins_with_barrier: bool,
}

/// The internal state of a `Submission` resource.
///
/// # Synchronisation model
///
/// `execute()` appends command buffers into *batches* (one batch per
/// queue).  `signal_semaphore()` marks a sync barrier: the next
/// `execute()` starts a fresh batch with `begins_with_barrier = true`.
///
/// At `flush()` time, binary semaphores are created **only** at barrier
/// boundaries and the batches are submitted with signal/wait chains:
///
/// ```text
/// batch₀ ──► signal(sem₀)
/// batch₁ ──► wait(sem₀)               (begins_with_barrier)
/// batch₂ ──► signal(sem₁)             (no barrier after — ends here)
/// batch₃ ──► wait(sem₁)               (begins_with_barrier)
/// ```
///
/// Batches without barriers between them are submitted independently
/// (no synchronisation).  All semaphore creation and submission happens
/// in `flush()` / `signal_fence_and_flush()`.
struct GpuSubmission {
    batches: Vec<Batch>,
    /// When true, the next `execute()` starts a new batch with
    /// `begins_with_barrier = true`.  Set by `signal_semaphore()`,
    /// cleared by `execute()`.
    force_new_batch: bool,
}

struct GpuFenceFuture {
    fence: vk::Fence,
    /// Semaphores owned by this future (destroyed on drop).
    semaphores: Vec<vk::Semaphore>,
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
        wait_semaphores: Vec<SemaphoreWaitInfo>,
        signal_semaphores: Vec<Resource<WitSemaphore>>,
        fence: Option<Resource<WitFence>>,
    ) -> Result<(), VulkanError> {
        let queue_key = Resource::<GpuQueue>::new_borrow(self_.rep());
        let gpu_queue = self
            .table
            .get(&queue_key)
            .map_err(|_| VulkanError::Unknown)?;

        let mut cmd_bufs_vk: Vec<vk::CommandBuffer> = Vec::new();
        let mut cmd_keys: Vec<Resource<WitCommandBuffer>> = Vec::new();
        for cb in &command_buffers {
            let cb_key = Resource::<super::command::GpuCommandBuffer>::new_borrow(cb.rep());
            let gpu_cb = self.table.get(&cb_key).map_err(|_| VulkanError::Unknown)?;
            cmd_bufs_vk.push(gpu_cb.cmd);
            cmd_keys.push(Resource::<WitCommandBuffer>::new_borrow(cb.rep()));
        }

        let mut wait_sems_vk: Vec<vk::Semaphore> = Vec::new();
        let mut wait_stages_vk: Vec<vk::PipelineStageFlags> = Vec::new();
        let mut sem_keys: Vec<Resource<WitSemaphore>> = Vec::new();
        for swi in &wait_semaphores {
            let sem_key = Resource::<super::sync::GpuSemaphore>::new_borrow(swi.semaphore.rep());
            let sem = self.table.get(&sem_key).map_err(|_| VulkanError::Unknown)?;
            wait_sems_vk.push(sem.semaphore);
            wait_stages_vk.push(vk::PipelineStageFlags::from_bits_truncate(swi.wait_stage));
            sem_keys.push(Resource::<WitSemaphore>::new_borrow(swi.semaphore.rep()));
        }

        let mut sig_sems_vk: Vec<vk::Semaphore> = Vec::new();
        let mut sig_keys: Vec<Resource<WitSemaphore>> = Vec::new();
        for ss in &signal_semaphores {
            let sem_key = Resource::<super::sync::GpuSemaphore>::new_borrow(ss.rep());
            let sem = self.table.get(&sem_key).map_err(|_| VulkanError::Unknown)?;
            sig_sems_vk.push(sem.semaphore);
            sig_keys.push(Resource::<WitSemaphore>::new_borrow(ss.rep()));
        }

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

        let submit_info = vk::SubmitInfo::builder()
            .wait_semaphores(&wait_sems_vk)
            .wait_dst_stage_mask(&wait_stages_vk)
            .command_buffers(&cmd_bufs_vk)
            .signal_semaphores(&sig_sems_vk);

        unsafe {
            self.vk_device()
                .queue_submit(gpu_queue.queue, &[submit_info.build()], fence_vk)
        }
        .map_err(vk_err)?;

        let _ = cmd_keys;
        let _ = sem_keys;
        let _ = sig_keys;

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

// ── Helpers ──

/// Create a VkQueueSubmit for a batch with optional wait/signal semaphores.
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
    unsafe { vk_dev.queue_submit(batch.queue, &[submit_info.build()], fence) }
        .map_err(vk_err)
}

/// Create a binary semaphore.
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

        // Start a new batch when `signal_semaphore()` was called (barrier)
        // or when the queue changes (different VkQueue = different submit).
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

        // Mark that the next execute() must start a fresh batch, creating a
        // sync barrier after the current batch.  Multiple consecutive
        // signal_semaphore() calls are coalesced — only one boundary is
        // inserted regardless of how many times it is called between
        // execute() calls.
        sub.force_new_batch = true;
    }

    fn flush(&mut self, self_: Resource<Submission>) -> Result<(), VulkanError> {
        let sub_key = Resource::<GpuSubmission>::new_own(self_.rep());
        let sub = self
            .table
            .delete(sub_key)
            .map_err(|_| VulkanError::Unknown)?;

        flush_impl(self, sub, vk::Fence::default(), /* return_semaphores */ false)
            .map(|_| ())
    }

    fn signal_fence_and_flush(
        &mut self,
        self_: Resource<Submission>,
    ) -> Result<Resource<FenceFuture>, VulkanError> {
        let sub_key = Resource::<GpuSubmission>::new_own(self_.rep());
        let sub = self
            .table
            .delete(sub_key)
            .map_err(|_| VulkanError::Unknown)?;

        // Create the fence first (before potentially panicking).
        let fence_info = vk::FenceCreateInfo::builder();
        let fence = unsafe { self.vk_device().create_fence(&fence_info, None) }.map_err(vk_err)?;

        if sub.batches.is_empty() {
            // No work — signal the fence immediately.
            unsafe { self.vk_device().destroy_fence(fence, None) };
            let signaled_info =
                vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED);
            let fence =
                unsafe { self.vk_device().create_fence(&signaled_info, None) }.map_err(vk_err)?;
            return finish_fence_future(self, fence, Vec::new());
        }

        let semaphores = flush_impl(self, sub, fence, /* return_semaphores */ true)?;
        finish_fence_future(self, fence, semaphores)
    }

    fn drop(&mut self, rep: Resource<Submission>) -> wasmtime::anyhow::Result<()> {
        let key = Resource::<GpuSubmission>::new_own(rep.rep());
        let sub = self.table.delete(key)?;
        // Command buffers were already deleted from the table in execute().
        // We don't free the VkCommandBuffers here — they belong to their pool.
        let _ = sub;
        Ok(())
    }
}

/// Core flush logic shared by `flush()` and `signal_fence_and_flush()`.
///
/// Creates binary semaphores **only** at boundaries marked by
/// `Batch::begins_with_barrier` (i.e. where `signal_semaphore()` was called
/// between consecutive `execute()` calls).  Adjacent batches without a barrier
/// are submitted independently — intra-queue ordering is guaranteed by the
/// queue itself, and cross-queue ordering is only enforced when explicitly
/// requested.
///
/// When `return_semaphores` is true, the created semaphores are returned
/// (for ownership by a `FenceFuture`).  When false, they are moved to
/// `pending_semaphores` for deferred cleanup.
fn flush_impl(
    ctx: &mut VkContextView<'_>,
    sub: GpuSubmission,
    final_fence: vk::Fence,
    return_semaphores: bool,
) -> Result<Vec<vk::Semaphore>, VulkanError> {
    // Collect non-empty batches only (empty batches are defensive — they
    // shouldn't occur in practice).
    let batches: Vec<&Batch> = sub.batches.iter().filter(|b| !b.cmds.is_empty()).collect();

    if batches.is_empty() {
        return Ok(Vec::new());
    }

    // Count how many barrier boundaries exist.
    let barrier_count = batches.iter().filter(|b| b.begins_with_barrier).count();

    // No barriers at all — every batch is independent.
    if barrier_count == 0 {
        for (i, batch) in batches.iter().enumerate() {
            let fence = if i == batches.len() - 1 { final_fence } else { vk::Fence::default() };
            let submit_info = vk::SubmitInfo::builder().command_buffers(&batch.cmds);
            unsafe {
                ctx.vk_device()
                    .queue_submit(batch.queue, &[submit_info.build()], fence)
            }
            .map_err(vk_err)?;
        }
        return Ok(Vec::new());
    }

    // Create one binary semaphore per barrier boundary.
    let vk_dev = ctx.vk_device();
    let semaphores: Vec<vk::Semaphore> = (0..barrier_count)
        .map(|_| unsafe { create_binary_semaphore(&vk_dev) })
        .collect::<Result<_, _>>()?;

    // Walk batches.  `next_sem` points to the next unconsumed semaphore.
    // A batch with `begins_with_barrier` *waits* on semaphore[next_sem].
    // A batch followed by a barrier boundary *signals* semaphore[next_sem]
    // (which will be waited on by the next barrier batch).
    let mut next_sem: usize = 0;

    for (i, batch) in batches.iter().enumerate() {
        // ── wait ──
        let (wait_slice, stage) = if batch.begins_with_barrier {
            let slice = &semaphores[next_sem..next_sem + 1];
            next_sem += 1; // consumed the wait
            (slice, vec![vk::PipelineStageFlags::ALL_COMMANDS])
        } else {
            (&[][..], vec![])
        };

        // ── signal ──
        let next_barrier = batches[i + 1..].iter().any(|b| b.begins_with_barrier);
        let signal_slice: &[vk::Semaphore] = if next_barrier {
            &semaphores[next_sem..next_sem + 1]
            // don't increment next_sem — the *waiter* will consume it
        } else {
            &[][..]
        };

        // ── fence ──
        // Only the last batch gets the caller's fence.
        let is_last = i == batches.len() - 1;
        let fence = if is_last { final_fence } else { vk::Fence::default() };

        unsafe { submit_batch(&vk_dev, batch, wait_slice, &stage, signal_slice, fence) }?;
    }

    if return_semaphores {
        Ok(semaphores)
    } else {
        ctx.owned
            .pending_semaphores
            .lock()
            .unwrap()
            .extend(semaphores);
        Ok(Vec::new())
    }
}

/// Push a `GpuFenceFuture` into the resource table and return it.
fn finish_fence_future(
    ctx: &mut VkContextView<'_>,
    fence: vk::Fence,
    semaphores: Vec<vk::Semaphore>,
) -> Result<Resource<FenceFuture>, VulkanError> {
    let handle = ctx
        .table
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
        let raw =
            unsafe { (self.owned.device_commands.get_fence_status)(self.owned.device, ff.fence) };
        match raw {
            vk::Result::SUCCESS => Ok(true),
            vk::Result::NOT_READY => Ok(false),
            _ => Err(vk_err(vk::ErrorCode::from(raw))),
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
