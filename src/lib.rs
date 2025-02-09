/*!
# **krnl**
Safe, portable, high performance compute (GPGPU) kernels.

Developed for [**autograph**](https://docs.rs/autograph).
- Similar functionality to CUDA and OpenCL.
- Supports GPU's and other Vulkan 1.2 capable devices.
- MacOS / iOS supported via [MoltenVK](https://github.com/KhronosGroup/MoltenVK).
- Kernels are written inline, entirely in Rust.
    - Simple iterator patterns can be implemented without unsafe.
    - Supports inline [SPIR-V](https://www.khronos.org/spir) assembly.
    - DebugPrintf integration, generates backtraces for panics.
- Buffers on the host can be accessed natively as Vecs and slices.

# **krnlc**
Kernel compiler for **krnl**.
- Built on [RustGPU](https://github.com/EmbarkStudios/rust-gpu)'s spirv-builder.
- Supports dependencies defined in Cargo.toml.
- Uses [spirv-tools](https://github.com/EmbarkStudios/spirv-tools-rs) to validate and optimize.
- Compiles to "krnl-cache.rs", so the crate will build on stable Rust.

See [krnlc](kernel#krnlc) for installation and usage instructions.

# Installing
For device functionality (kernels), install [Vulkan](https://www.vulkan.org) for your platform.
- For development, it's recomended to install the [LunarG Vulkan SDK](https://www.lunarg.com/vulkan-sdk/), which includes additional tools:
    - vulkaninfo
    - Validation layers
        - DebugPrintf
    - spirv-tools
        - This is used by **krnlc** for spirv validation and optimization.
            - **krnlc** builds by default without needing spirv-tools to be installed.

## Test
- Check that `vulkaninfo --summary` shows your devices.
    - Instance version should be >= 1.2.
- Alternatively, check that `cargo test --test integration_tests -- --exact none` shows your devices.
    - You can run all the tests with `cargo test`.

# Getting Started
- See [device] for creating devices.
- See [buffer] for creating buffers.
- See [kernel] for compute kernels.

# Example
```no_run
use krnl::{
    macros::module,
    anyhow::Result,
    device::Device,
    buffer::{Buffer, Slice, SliceMut},
};

#[module]
# #[krnl(no_build)]
mod kernels {
    #[cfg(not(target_arch = "spirv"))]
    use krnl::krnl_core;
    use krnl_core::macros::kernel;

    pub fn saxpy_impl(alpha: f32, x: f32, y: &mut f32) {
        *y += alpha * x;
    }

    // Item kernels for iterator patterns.
    #[kernel]
    pub fn saxpy(alpha: f32, #[item] x: f32, #[item] y: &mut f32) {
        saxpy_impl(alpha, x, y);
    }

    // General purpose kernels like CUDA / OpenCL.
    #[kernel]
    pub fn saxpy_global(alpha: f32, #[global] x: Slice<f32>, #[global] y: UnsafeSlice<f32>) {
        use krnl_core::buffer::UnsafeIndex;

        let global_id = kernel.global_id();
        if global_id < x.len().min(y.len()) {
            saxpy_impl(alpha, x[global_id], unsafe { y.unsafe_index_mut(global_id) });
        }
    }
}

fn saxpy(alpha: f32, x: Slice<f32>, mut y: SliceMut<f32>) -> Result<()> {
    if let Some((x, y)) = x.as_host_slice().zip(y.as_host_slice_mut()) {
        x.iter()
            .copied()
            .zip(y.iter_mut())
            .for_each(|(x, y)| kernels::saxpy_impl(alpha, x, y));
        return Ok(());
    }
    # if true {
    kernels::saxpy::builder()?
        .build(y.device())?
        .dispatch(alpha, x, y)
    # } else {
    // or
    kernels::saxpy_global::builder()?
        .build(y.device())?
        .with_global_threads(y.len() as u32)
        .dispatch(alpha, x, y)
    # }
}

fn main() -> Result<()> {
    let alpha = 2f32;
    let x = vec![1f32];
    let y = vec![0f32];
    let device = Device::builder().build().ok().unwrap_or(Device::host());
    let x = Buffer::from(x).into_device(device.clone())?;
    let mut y = Buffer::from(y).into_device(device.clone())?;
    saxpy(alpha, x.as_slice(), y.as_slice_mut())?;
    let y = y.into_vec()?;
    println!("{y:?}");
    Ok(())
}
```
*/

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

/// anyhow
pub extern crate anyhow;
/// krnl-core
pub extern crate krnl_core;
/// krnl-macros
pub extern crate krnl_macros as macros;

/// half
pub use krnl_core::half;

#[doc(inline)]
pub use krnl_core::scalar;

/// Buffers.
pub mod buffer;
/// Devices.
pub mod device;
/// Kernels.
pub mod kernel;
