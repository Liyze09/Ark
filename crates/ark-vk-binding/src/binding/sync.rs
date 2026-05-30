use vulkanalia::vk;

use crate::
    binding::ark::gpu::{
        sync::SharingMode,
        queue::QueueFamily,
    }
;

pub fn vk_sharing_mode(mode: &Option<SharingMode>) -> (vk::SharingMode, Vec<u32>) {
    match mode {
        None | Some(SharingMode::Exclusive) => (vk::SharingMode::EXCLUSIVE, vec![]),
        Some(SharingMode::Concurrent(families)) => {
            let indices: Vec<u32> = families.iter().map(|qf| match qf {
                QueueFamily::Graphics => 0,
                QueueFamily::Compute => 1,
                QueueFamily::Transfer => 2,
            }).collect();
            (vk::SharingMode::CONCURRENT, indices)
        }
    }
}