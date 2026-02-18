use std::{sync::{Arc, Mutex}, thread::{sleep, yield_now}};

use anyhow::{Ok, anyhow};
use enum_map::{Enum, EnumMap};
use vulkano::sync::{GpuFuture, future::{FenceSignalFuture, JoinFuture}};

#[derive(Default, Clone)]
pub struct RenderStateManager {
    pub states: Arc<EnumMap<RenderState, Mutex<PassState>>>
}

unsafe impl Send for RenderStateManager {}
unsafe impl Sync for RenderStateManager {}

pub trait StateFuture: GpuFuture {
    fn state_wait(&self) -> anyhow::Result<()>;
}

impl<F: GpuFuture> StateFuture for FenceSignalFuture<F> {
    fn state_wait(&self) -> anyhow::Result<()> {
        self.wait(None)?;
        Ok(())
    }
}


#[derive(Default, Enum, Copy, Clone)]
pub enum RenderState {
    #[default]
    Idle,
    ChunkUploading,
    Rendering
}

#[derive(Default)]
pub enum PassState {
    #[default]
    Empty,
    CpuWorking,
    GpuWorking(Box<dyn StateFuture>),
}

impl RenderStateManager {
    pub fn acquire(&self, state: RenderState) -> anyhow::Result<()> {
        loop {
            let mut guard = self.states[state].lock().map_err(|err| anyhow!("{}", err))?;
            match &mut *guard {
                PassState::Empty => {
                    *guard = PassState::CpuWorking;
                    break;
                }
                PassState::CpuWorking => {
                    drop(guard);
                    yield_now();
                }
                PassState::GpuWorking(future) => {
                    if let Err(e) = future.state_wait() {
                        *guard = PassState::Empty;
                        return Err(e);
                    }
                    *guard = PassState::CpuWorking;
                    break;
                }
            }
        }
        Ok(())
    }

    pub fn release(&self, state: RenderState, future: Option<Box<dyn StateFuture>>) -> anyhow::Result<()> {
        let mut value = self.states[state].lock().map_err(|err| anyhow!("{}", err))?;
        match &*value {
            PassState::CpuWorking => {}
            _ => return Err(anyhow!("State is not owned by this context"))
        }
        *value = match future {
            Some(future) => PassState::GpuWorking(future),
            None => PassState::Empty
        };
        Ok(())
    }

    pub fn wait(&self, state: RenderState) -> anyhow::Result<()> {
        loop {
            let mut guard = self.states[state].lock().map_err(|err| anyhow!("{}", err))?;
            match &mut *guard {
                PassState::Empty => {
                    break;
                }
                PassState::CpuWorking => {
                    drop(guard);
                    yield_now();
                }
                PassState::GpuWorking(future) => {
                    if let Err(e) = future.state_wait() {
                        *guard = PassState::Empty;
                        return Err(e);
                    }
                    *guard = PassState::Empty;
                    break;
                }
            }
        }
        Ok(())
    }

    pub fn signal(&self, state: RenderState, future: Box<dyn StateFuture>) -> anyhow::Result<()> {
        loop {
            let mut guard = self.states[state].lock().map_err(|err| anyhow!("{}", err))?;
            match &mut *guard {
                PassState::Empty => {
                    *guard = PassState::GpuWorking(future);
                    break;
                }
                PassState::CpuWorking => {
                    drop(guard);
                    yield_now();
                }
                PassState::GpuWorking(future_old) => {
                    if let Err(e) = future_old.state_wait() {
                        *guard = PassState::GpuWorking(future);
                        return Err(e);
                    }
                    *guard = PassState::GpuWorking(future);
                    break;
                }
            }
        }
        Ok(())
    }
}
